//! Shared sandbox for file-based Rhai trigger scripts (`scripts/triggers/*.rhai`).
//!
//! Every place that runs a trigger script goes through this module:
//!
//! - [`read_trigger_script`] validates the script name (no path traversal: a
//!   builder-supplied `script_name` of `../commands/admin` used to load and run
//!   an arbitrary `.rhai` file) and caches the source by mtime, so hot paths
//!   such as `on_prompt` don't hit the disk on every prompt.
//! - [`sandboxed_engine`] builds the per-fire engine with resource limits, no
//!   `eval`, and no module imports. Several call sites used a bare
//!   `Engine::new()` with no limits, so an infinite loop in a trigger script
//!   hung the tick (or, for `on_prompt`, the World lock) forever.
//! - [`guard_panic`] turns a panic inside a native function into a script
//!   error so it can't unwind out of a tick or a connection task.
//!
//! Call sites still register their own context-specific functions on the
//! engine; only construction, loading and the call boundary are shared.

use parking_lot::Mutex;
use rhai::{Engine, EvalAltResult};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::SystemTime;

/// Directory trigger scripts are loaded from.
pub const TRIGGER_SCRIPT_DIR: &str = "scripts/triggers";

/// Rhai operation budget per trigger call.
pub const MAX_OPERATIONS: u64 = 100_000;
/// Longest string a trigger script may build.
pub const MAX_STRING_SIZE: usize = 100_000;
/// Largest array / map a trigger script may build.
pub const MAX_COLLECTION_SIZE: usize = 1_000;
/// Maximum function call nesting inside a trigger script.
pub const MAX_CALL_LEVELS: usize = 64;

/// True when `name` is a safe trigger script name: ASCII alphanumerics, `_`
/// and `-` only (an optional leading `@` marks a built-in template). Anything
/// else (`/`, `.`, `\`) could escape [`TRIGGER_SCRIPT_DIR`].
pub fn is_valid_script_name(name: &str) -> bool {
    let check = name.strip_prefix('@').unwrap_or(name);
    !check.is_empty() && check.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Path of the trigger script `name`, or `None` if the name is invalid or a
/// built-in `@template` (which has no file).
pub fn trigger_script_path(name: &str) -> Option<PathBuf> {
    if name.starts_with('@') || !is_valid_script_name(name) {
        return None;
    }
    Some(PathBuf::from(format!("{TRIGGER_SCRIPT_DIR}/{name}.rhai")))
}

type SourceCache = Mutex<HashMap<PathBuf, (SystemTime, String)>>;

fn source_cache() -> &'static SourceCache {
    static CACHE: OnceLock<SourceCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Read the source of trigger script `name`.
///
/// Returns an `InvalidInput` error for an unsafe name, so callers' existing
/// "failed to load" handling covers it. Sources are cached and re-read only
/// when the file's mtime changes.
pub fn read_trigger_script(name: &str) -> std::io::Result<String> {
    let Some(path) = trigger_script_path(name) else {
        tracing::warn!("[SECURITY] Blocked trigger script with invalid name: {:?}", name);
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid trigger script name: {name:?}"),
        ));
    };
    let mtime = std::fs::metadata(&path)?.modified()?;
    {
        let cache = source_cache().lock();
        if let Some((cached_mtime, src)) = cache.get(&path) {
            if *cached_mtime == mtime {
                return Ok(src.clone());
            }
        }
    }
    let src = std::fs::read_to_string(&path)?;
    source_cache().lock().insert(path, (mtime, src.clone()));
    Ok(src)
}

/// A fresh engine for running one trigger script: resource limits, `eval`
/// disabled, and module imports refused.
pub fn sandboxed_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(128, 128);
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_COLLECTION_SIZE);
    engine.set_max_map_size(MAX_COLLECTION_SIZE);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.disable_symbol("eval");
    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine
}

/// Run `f` (typically an `engine.call_fn(..)`), converting a panic in any
/// native function it reaches into a runtime error instead of unwinding.
pub fn guard_panic<T>(f: impl FnOnce() -> Result<T, Box<EvalAltResult>>) -> Result<T, Box<EvalAltResult>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let detail = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("trigger script panicked: {}", detail);
            Err(Box::new(EvalAltResult::ErrorRuntime(
                format!("internal error: {detail}").into(),
                rhai::Position::NONE,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_names_are_rejected() {
        for bad in ["../commands/admin", "a/b", "..", "x.rhai", "a\\b", "", "@", "a b"] {
            assert!(trigger_script_path(bad).is_none(), "{bad:?} should be rejected");
            let err = read_trigger_script(bad).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
        }
        assert!(trigger_script_path("crystal_greet").is_some());
        assert!(trigger_script_path("my-trigger_2").is_some());
        // Templates are valid names but have no file.
        assert!(is_valid_script_name("@say"));
        assert!(trigger_script_path("@say").is_none());
    }

    #[test]
    fn infinite_loop_is_stopped_by_the_operation_budget() {
        let engine = sandboxed_engine();
        let err = engine.eval::<()>("loop { }").unwrap_err();
        assert!(matches!(*err, EvalAltResult::ErrorTooManyOperations(_)), "{err}");
    }

    #[test]
    fn eval_and_import_are_unavailable() {
        let engine = sandboxed_engine();
        assert!(engine.compile(r#"eval("1 + 1")"#).is_err(), "eval must not compile");
        let err = engine.eval::<()>(r#"import "x" as y;"#).unwrap_err();
        assert!(matches!(*err, EvalAltResult::ErrorModuleNotFound(..)), "{err}");
    }

    #[test]
    fn string_growth_is_capped() {
        let engine = sandboxed_engine();
        let err = engine
            .eval::<()>(r#"let s = "aaaaaaaaaa"; loop { s += s; }"#)
            .unwrap_err();
        assert!(
            matches!(*err, EvalAltResult::ErrorDataTooLarge(..)),
            "expected size limit, got {err}"
        );
    }

    #[test]
    fn every_shipped_trigger_script_compiles_in_the_sandbox() {
        let mut failures = Vec::new();
        for entry in std::fs::read_dir(TRIGGER_SCRIPT_DIR).expect("read trigger dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rhai") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            match read_trigger_script(&name) {
                Ok(src) => {
                    if let Err(e) = sandboxed_engine().compile(&src) {
                        failures.push(format!("{name}: {e}"));
                    }
                }
                Err(e) => failures.push(format!("{name}: unreadable via loader: {e}")),
            }
        }
        assert!(
            failures.is_empty(),
            "trigger scripts that fail in the sandbox:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn a_panicking_native_fn_becomes_an_error() {
        let mut engine = sandboxed_engine();
        engine.register_fn("boom", || -> i64 { panic!("native panic") });
        let ast = engine.compile("fn run_trigger() { boom() }").unwrap();
        let r = guard_panic(|| engine.call_fn::<i64>(&mut rhai::Scope::new(), &ast, "run_trigger", ()));
        assert!(r.is_err());
    }
}

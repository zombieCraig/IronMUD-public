//! Guards against two registration steps registering the same Rhai function
//! signature. Rhai silently keeps the later registration, so a duplicate in an
//! unrelated module can override a carefully-guarded function. Past bugs:
//! an unguarded `random_int` (panics on min > max) overriding the guarded one,
//! and a legacy `add_character_gold` bypassing the purse chokepoint.

use ironmud::script::registration_steps;
use parking_lot::Mutex;
use rhai::Engine;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Signatures that are intentionally registered by more than one step.
/// Every entry needs a justification. Keep this list empty if you can.
const ALLOWED_DUPLICATES: &[&str] = &[];

#[test]
fn no_rhai_function_is_registered_by_two_steps() {
    let temp = tempfile::tempdir().expect("temp dir");
    let db = ironmud::db::Db::open(temp.path()).expect("open db");
    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    let state = ironmud::World::minimal_shared(db.clone(), connections.clone());

    let mut owners: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for (name, step) in registration_steps(Arc::new(db), connections, state) {
        let mut scratch = Engine::new_raw();
        step(&mut scratch);
        for sig in scratch.gen_fn_signatures(false) {
            owners.entry(sig).or_default().push(name);
        }
    }

    let dupes: Vec<String> = owners
        .iter()
        .filter(|(sig, steps)| steps.len() > 1 && !ALLOWED_DUPLICATES.contains(&sig.as_str()))
        .map(|(sig, steps)| format!("  {sig}  <- {}", steps.join(", ")))
        .collect();
    assert!(
        dupes.is_empty(),
        "Rhai functions registered by more than one step (the LAST one silently wins):\n{}",
        dupes.join("\n")
    );
}

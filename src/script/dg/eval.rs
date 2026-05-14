//! DG Scripts evaluator. Tree-walks the AST against an [`EvalCtx`].
//!
//! ## Condition language
//!
//! Conditions in `if`/`elseif`/`while` are stored as raw strings and
//! evaluated by [`eval_cond`]. We support the subset that covers >95% of
//! stock tbamud triggers:
//!
//! ```text
//! <expr> <op> <expr>          ==, !=, /=, <, >, <=, >=, =~, !~
//! <bool> && <bool>
//! <bool> || <bool>
//! ! <bool>
//! ( <bool> )
//! ```
//!
//! Each `<expr>` is variable-substituted first, then compared as integer
//! when both sides parse as integers, otherwise as strings (DG semantics).
//!
//! ## Loop / recursion safety
//!
//! - `while` bodies are capped at [`LOOP_CAP`] iterations.
//! - Nested block depth is bounded by Rust's stack — adversarial scripts
//!   could OOM, but stock tbamud nests at most ~6 levels.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use uuid::Uuid;

use super::{Outcome, EvalCtx};
use super::ast::{Block, Stmt, SwitchCase};
use super::vars;

/// Hard cap on iterations of any single `while` loop. Matches tbamud's
/// per-pulse cap (30) — stock scripts never legitimately loop more.
const LOOP_CAP: u32 = 30;

#[derive(Debug)]
pub struct EvalError {
    pub msg: String,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DG eval error: {}", self.msg)
    }
}

impl std::error::Error for EvalError {}

/// Identifies an entity that owns a durable `dg_vars` map. `remote` /
/// `rdelete` / `context` accept either a UUID (mob/item/room) or a
/// character name (PCs are keyed by name, not UUID, in IronMUD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeRef {
    /// Mobile, item, or room — looked up by UUID.
    Uuid(Uuid),
    /// Player character — looked up by lowercase name.
    Player(String),
}

/// Per-script mutable state. Created fresh on each `fire_dg` call.
///
/// Holds the variable scopes the DG model exposes:
/// - `locals` — `set <var> <val>` writes here. Lives only for this run.
/// - `globals` — names previously declared `global <var>`. A read of
///   `%var%` whose name is in this set bypasses `locals` and goes to
///   the durable store (world `dg_globals` if `context` is None, the
///   context entity's `dg_vars` otherwise).
/// - `context` — current `context %expr%` binding. `None` means world
///   scope; `Some(ScopeRef)` means subsequent `global`/`unset` writes
///   target that entity's `dg_vars` (works for PCs too — IronMUD-specific).
#[derive(Debug, Default)]
pub struct State {
    pub locals: HashMap<String, String>,
    pub globals: HashSet<String>,
    pub context: Option<ScopeRef>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Internal control-flow signal for the recursive evaluator.
enum Flow {
    Continue,
    Halt,
    Return(i32),
    Break,
}

pub fn eval_block(block: &Block, ctx: &EvalCtx, state: &mut State) -> Result<Outcome, EvalError> {
    match eval_stmts(block, ctx, state)? {
        Flow::Continue => Ok(Outcome::Done),
        Flow::Halt => Ok(Outcome::Halt),
        Flow::Return(n) => Ok(Outcome::Return(n)),
        // Top-level `break` is a no-op (DG ignores it outside switch/while).
        Flow::Break => Ok(Outcome::Done),
    }
}

fn eval_stmts(stmts: &[Stmt], ctx: &EvalCtx, state: &mut State) -> Result<Flow, EvalError> {
    for s in stmts {
        match eval_stmt(s, ctx, state)? {
            Flow::Continue => {}
            other => return Ok(other),
        }
    }
    Ok(Flow::Continue)
}

fn eval_stmt(stmt: &Stmt, ctx: &EvalCtx, state: &mut State) -> Result<Flow, EvalError> {
    match stmt {
        Stmt::Comment(_) => Ok(Flow::Continue),
        // `nop` evaluates its argument but produces no output — used to
        // fire side-effecting interpolations like `%actor.gold(-N)%` when
        // the writer is just trying to mutate state.
        Stmt::Nop(s) => {
            let _ = vars::substitute(s, ctx, state);
            Ok(Flow::Continue)
        }
        Stmt::Halt => Ok(Flow::Halt),
        Stmt::Return(n) => Ok(Flow::Return(*n)),
        Stmt::Break => Ok(Flow::Break),

        // Wait is handled by the async eval path. The sync evaluator
        // treats it as a no-op so triggers without time-sensitive logic
        // still complete; bodies that contain a `wait` should be routed
        // to `eval_block_async` by `fire_dg` before reaching here.
        Stmt::Wait(_) => Ok(Flow::Continue),

        Stmt::Attach { trig_vnum, target } => {
            let vnum = vars::substitute(trig_vnum.trim(), ctx, state);
            let tgt = vars::substitute(target.trim(), ctx, state);
            super::cmds::attach_trigger_proto(&vnum, &tgt, ctx);
            Ok(Flow::Continue)
        }

        Stmt::Detach { trig_vnum, target } => {
            let vnum = vars::substitute(trig_vnum.trim(), ctx, state);
            let tgt = vars::substitute(target.trim(), ctx, state);
            super::cmds::detach_trigger_proto(&vnum, &tgt, ctx);
            Ok(Flow::Continue)
        }

        Stmt::Context(arg) => {
            let interp = vars::substitute(arg.trim(), ctx, state);
            state.context = parse_scope_ref(&interp);
            Ok(Flow::Continue)
        }

        Stmt::Global(var) => {
            let name = var.trim().to_string();
            if name.is_empty() {
                return Ok(Flow::Continue);
            }
            // Promote: copy current local value (if any) to the durable
            // store, and remember the name so future reads prefer the
            // durable lookup.
            if let Some(val) = state.locals.get(&name).cloned() {
                store_durable(ctx, state, &name, &val);
            }
            state.globals.insert(name);
            Ok(Flow::Continue)
        }

        Stmt::Unset(var) => {
            let name = var.trim();
            if name.is_empty() {
                return Ok(Flow::Continue);
            }
            state.locals.remove(name);
            state.globals.remove(name);
            // Clear durable too — broadcast unset is the principle of
            // least-surprise; otherwise an old global value would shadow
            // a new local.
            let _ = ctx.db.unset_dg_global(name);
            if let Some(scope) = state.context.clone() {
                clear_entity_var(ctx, &scope, name);
            }
            Ok(Flow::Continue)
        }

        Stmt::Remote { var, target } => {
            let var_name = var.trim();
            if var_name.is_empty() {
                return Ok(Flow::Continue);
            }
            let interp = vars::substitute(target.trim(), ctx, state);
            let Some(scope) = parse_scope_ref(&interp) else {
                return Ok(Flow::Continue);
            };
            // Source value: the current scope's value of var (locals first,
            // then durable lookup if globalled).
            let value = vars::resolve(var_name, ctx, state);
            set_entity_var(ctx, &scope, var_name, &value);
            Ok(Flow::Continue)
        }

        Stmt::Rdelete { var, target } => {
            let var_name = var.trim();
            if var_name.is_empty() {
                return Ok(Flow::Continue);
            }
            let interp = vars::substitute(target.trim(), ctx, state);
            let Some(scope) = parse_scope_ref(&interp) else {
                return Ok(Flow::Continue);
            };
            clear_entity_var(ctx, &scope, var_name);
            Ok(Flow::Continue)
        }

        Stmt::Set { var, value } => {
            let v = vars::substitute(value, ctx, state);
            // If this name is in `globals`, also write through to the
            // durable store so subsequent reads see the update without a
            // re-`global` declaration.
            if state.globals.contains(var) {
                store_durable(ctx, state, var, &v);
            }
            state.locals.insert(var.clone(), v);
            Ok(Flow::Continue)
        }

        Stmt::Eval { var, expr } => {
            // Phase 1: evaluate as substituted-string-with-arithmetic.
            // Stock tbamud uses `eval` mostly for var copies (`eval x %y%`)
            // and simple arithmetic (`eval i %i% + 1`).
            let raw = vars::substitute(expr, ctx, state);
            let result = eval_expr(&raw);
            if state.globals.contains(var) {
                store_durable(ctx, state, var, &result);
            }
            state.locals.insert(var.clone(), result);
            Ok(Flow::Continue)
        }

        Stmt::Cmd(line) => {
            let interp = vars::substitute(line, ctx, state);
            super::cmds::dispatch(&interp, ctx).map_err(|m| EvalError { msg: m })?;
            Ok(Flow::Continue)
        }

        Stmt::If { cond, then_body, elif_branches, else_body } => {
            if eval_cond(cond, ctx, state) {
                return eval_stmts(then_body, ctx, state);
            }
            for (c, body) in elif_branches {
                if eval_cond(c, ctx, state) {
                    return eval_stmts(body, ctx, state);
                }
            }
            if let Some(body) = else_body {
                return eval_stmts(body, ctx, state);
            }
            Ok(Flow::Continue)
        }

        Stmt::While { cond, body } => {
            let mut iters: u32 = 0;
            while eval_cond(cond, ctx, state) {
                if iters >= LOOP_CAP {
                    tracing::warn!("DG while loop hit LOOP_CAP={} on '{}'", LOOP_CAP, ctx.self_name);
                    break;
                }
                match eval_stmts(body, ctx, state)? {
                    Flow::Continue => {}
                    Flow::Break => break,
                    other => return Ok(other),
                }
                iters += 1;
            }
            Ok(Flow::Continue)
        }

        Stmt::Switch { value, cases, default } => {
            let head = vars::substitute(value, ctx, state);
            let head_trim = head.trim();
            let mut matched: Option<usize> = None;
            for (i, c) in cases.iter().enumerate() {
                if c.patterns.iter().any(|p| {
                    let pv = vars::substitute(p, ctx, state);
                    pv.trim() == head_trim
                }) {
                    matched = Some(i);
                    break;
                }
            }
            if let Some(start) = matched {
                // Execute from `start` onward, honoring fall_through.
                for c in &cases[start..] {
                    match exec_switch_case(c, ctx, state)? {
                        Flow::Continue => {}
                        Flow::Break => return Ok(Flow::Continue),
                        other => return Ok(other),
                    }
                    if !c.fall_through {
                        return Ok(Flow::Continue);
                    }
                }
                // Fell through past the last case — also try default.
                if let Some(d) = default {
                    return run_case_body(d, ctx, state);
                }
            } else if let Some(d) = default {
                return run_case_body(d, ctx, state);
            }
            Ok(Flow::Continue)
        }
    }
}

fn exec_switch_case(c: &SwitchCase, ctx: &EvalCtx, state: &mut State) -> Result<Flow, EvalError> {
    run_case_body(&c.body, ctx, state)
}

fn run_case_body(body: &Block, ctx: &EvalCtx, state: &mut State) -> Result<Flow, EvalError> {
    eval_stmts(body, ctx, state)
}

// ---------- Condition evaluation ----------

/// Evaluate a DG condition string. Returns `true` if the condition is
/// truthy. Parser is forgiving: malformed conditions evaluate as `false`
/// (matches tbamud's silent-error behavior).
pub fn eval_cond(cond: &str, ctx: &EvalCtx, state: &State) -> bool {
    let interp = vars::substitute(cond.trim(), ctx, state);
    eval_bool(&interp)
}

/// Boolean evaluator over an already-substituted expression string.
///
/// Recursive-descent over `||` (lowest precedence), `&&`, `!`, and
/// finally comparison/atom.
fn eval_bool(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    // Handle `||`
    if let Some((l, r)) = split_top_level(s, "||") {
        return eval_bool(&l) || eval_bool(&r);
    }
    if let Some((l, r)) = split_top_level(s, "&&") {
        return eval_bool(&l) && eval_bool(&r);
    }
    // Leading `!`
    if let Some(rest) = s.strip_prefix('!') {
        // But careful not to confuse with `!=` / `!~`.
        if !rest.starts_with('=') && !rest.starts_with('~') {
            return !eval_bool(rest);
        }
    }
    // Parenthesized
    if s.starts_with('(') && s.ends_with(')') {
        // Only strip if the parens are matched at top level.
        if matched_outer_parens(s) {
            return eval_bool(&s[1..s.len() - 1]);
        }
    }
    eval_compare(s)
}

/// Find an operator at the top level (outside parens), returning the
/// (left, right) split. Used for short-circuit ops `||` and `&&`.
fn split_top_level(s: &str, op: &str) -> Option<(String, String)> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let op_bytes = op.as_bytes();
    let n = op_bytes.len();
    let mut i = 0;
    while i + n <= bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && bytes[i..i + n] == *op_bytes {
            // Avoid matching `&&` inside `&` token? DG doesn't have a single
            // `&` so this is fine.
            let l = s[..i].to_string();
            let r = s[i + n..].to_string();
            return Some((l, r));
        }
        i += 1;
    }
    None
}

fn matched_outer_parens(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return false;
    }
    let mut depth = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 && i + 1 < bytes.len() {
                return false;
            }
        }
    }
    depth == 0
}

/// Evaluate a comparison expression `<lhs> <op> <rhs>` or a bare value
/// (truthy if non-empty and non-zero).
fn eval_compare(s: &str) -> bool {
    // Operator order matters: `==`, `!=`, `/=`, `<=`, `>=`, `=~`, `!~`,
    // then single-char `<`, `>`. Search top-level only.
    for op in &["==", "!=", "/=", "<=", ">=", "=~", "!~"] {
        if let Some((l, r)) = split_top_level(s, op) {
            return apply_cmp(op, &l, &r);
        }
    }
    for op in &["<", ">"] {
        if let Some((l, r)) = split_top_level(s, op) {
            return apply_cmp(op, &l, &r);
        }
    }
    // Bare value — truthy if non-empty and not "0".
    let t = s.trim();
    !(t.is_empty() || t == "0")
}

fn apply_cmp(op: &str, l: &str, r: &str) -> bool {
    let l = l.trim();
    let r = r.trim();
    // Try numeric comparison first.
    let li = l.parse::<i64>().ok();
    let ri = r.parse::<i64>().ok();
    if let (Some(li), Some(ri)) = (li, ri) {
        return match op {
            "==" => li == ri,
            "!=" | "/=" => li != ri,
            "<" => li < ri,
            ">" => li > ri,
            "<=" => li <= ri,
            ">=" => li >= ri,
            "=~" => l.contains(r),
            "!~" => !l.contains(r),
            _ => false,
        };
    }
    // String comparison fallback (case-insensitive for /= per tbamud).
    let lc = l.to_ascii_lowercase();
    let rc = r.to_ascii_lowercase();
    match op {
        "==" => lc == rc,
        "!=" => lc != rc,
        // `/=` is case-insensitive equality OR keyword-prefix match in
        // tbamud, depending on context. We use case-insensitive equality;
        // command-trigger keyword matching uses `=~`.
        "/=" => lc == rc || lc.starts_with(&rc) || rc.starts_with(&lc),
        "=~" => lc.contains(&rc),
        "!~" => !lc.contains(&rc),
        "<" => lc < rc,
        ">" => lc > rc,
        "<=" => lc <= rc,
        ">=" => lc >= rc,
        _ => false,
    }
}

/// Parse a DG `context` argument into a UUID. Empty / "0" / unparseable
/// values clear the context (return None), matching the convention of
/// `context 0` meaning "world scope".
/// Parse the interpolated target of `remote`/`rdelete`/`context` into a
/// [`ScopeRef`]. UUID strings resolve to [`ScopeRef::Uuid`] (mob/item/room);
/// any other non-empty value is treated as a character name (PCs are keyed
/// by name, not UUID). `""` and `"0"` mean "no scope".
fn parse_scope_ref(s: &str) -> Option<ScopeRef> {
    let t = s.trim();
    if t.is_empty() || t == "0" {
        return None;
    }
    if let Ok(u) = Uuid::parse_str(t) {
        return Some(ScopeRef::Uuid(u));
    }
    Some(ScopeRef::Player(t.to_ascii_lowercase()))
}

/// Write `name = value` into the durable scope: world `dg_globals` when
/// no context is set, the context entity's `dg_vars` otherwise.
fn store_durable(ctx: &EvalCtx, state: &State, name: &str, value: &str) {
    match state.context.as_ref() {
        None => {
            let _ = ctx.db.set_dg_global(name, value);
        }
        Some(scope) => set_entity_var(ctx, scope, name, value),
    }
}

/// Set `name = value` on the scope's `dg_vars`. For UUID scopes, tries
/// mobiles → items → rooms. For player scopes, writes to the character's
/// `dg_vars` keyed by lowercase name.
pub(super) fn set_entity_var(ctx: &EvalCtx, scope: &ScopeRef, name: &str, value: &str) {
    match scope {
        ScopeRef::Uuid(uid) => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(uid) {
                mob.dg_vars.insert(name.to_string(), value.to_string());
                let _ = ctx.db.save_mobile_data(mob);
                return;
            }
            if let Ok(Some(mut item)) = ctx.db.get_item_data(uid) {
                item.dg_vars.insert(name.to_string(), value.to_string());
                let _ = ctx.db.save_item_data(item);
                return;
            }
            if let Ok(Some(mut room)) = ctx.db.get_room_data(uid) {
                room.dg_vars.insert(name.to_string(), value.to_string());
                let _ = ctx.db.save_room_data(room);
            }
        }
        ScopeRef::Player(pname) => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(pname) {
                ch.dg_vars.insert(name.to_string(), value.to_string());
                let _ = ctx.db.save_character_data(ch);
            }
        }
    }
}

/// Remove `name` from the scope's `dg_vars`. Mirror of [`set_entity_var`].
pub(super) fn clear_entity_var(ctx: &EvalCtx, scope: &ScopeRef, name: &str) {
    match scope {
        ScopeRef::Uuid(uid) => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(uid) {
                if mob.dg_vars.remove(name).is_some() {
                    let _ = ctx.db.save_mobile_data(mob);
                }
                return;
            }
            if let Ok(Some(mut item)) = ctx.db.get_item_data(uid) {
                if item.dg_vars.remove(name).is_some() {
                    let _ = ctx.db.save_item_data(item);
                }
                return;
            }
            if let Ok(Some(mut room)) = ctx.db.get_room_data(uid) {
                if room.dg_vars.remove(name).is_some() {
                    let _ = ctx.db.save_room_data(room);
                }
            }
        }
        ScopeRef::Player(pname) => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(pname) {
                if ch.dg_vars.remove(name).is_some() {
                    let _ = ctx.db.save_character_data(ch);
                }
            }
        }
    }
}

/// Convert a `wait` argument like `"3 sec"`, `"1 min"`, `"30"` (DG
/// pulses, ~0.1s each), or `"3"` (bare seconds in some scripts) into a
/// real-second delay. Returns 0 for unparseable input — the eval treats
/// 0 as "no wait" and continues immediately.
pub fn parse_wait_secs(s: &str) -> u64 {
    let t = s.trim().to_ascii_lowercase();
    if t.is_empty() {
        return 0;
    }
    let mut tokens = t.split_whitespace();
    let n_tok = tokens.next().unwrap_or("0");
    let unit = tokens.next().unwrap_or("");
    let n: u64 = n_tok.parse().unwrap_or(0);
    match unit {
        "" => {
            // Bare number — tbamud convention is "pulses" (0.1s each)
            // but most stock triggers use `wait 3 sec` explicitly. Treat
            // bare numbers as seconds for usability.
            n
        }
        "s" | "sec" | "second" | "seconds" => n,
        "m" | "min" | "minute" | "minutes" => n * 60,
        "h" | "hr" | "hour" | "hours" => n * 3600,
        // "wait until 6:00" — Phase 2+ time-of-day wait. Treat as 0
        // (proceed immediately) for now.
        _ => 0,
    }
}

/// Expression evaluator for `eval`. Supports integer arithmetic with
/// parentheses, unary minus, and full +-*/% precedence on already-
/// substituted text. Falls back to returning the raw substituted string
/// when arithmetic doesn't parse cleanly (e.g. `eval x %y%`).
pub(crate) fn eval_expr(s: &str) -> String {
    match parse_arith(s) {
        Some(v) => v.to_string(),
        None => s.trim().to_string(),
    }
}

/// Best-effort integer parse of an arithmetic expression. Returns `None`
/// when the input has any non-numeric atoms, leftover characters, or
/// division/mod by zero — caller treats that as "not arithmetic".
pub(crate) fn parse_arith(s: &str) -> Option<i64> {
    let mut p = ArithParser { bytes: s.as_bytes(), pos: 0 };
    let v = p.parse_addsub()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return None;
    }
    Some(v)
}

struct ArithParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ArithParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn parse_addsub(&mut self) -> Option<i64> {
        let mut lhs = self.parse_muldiv()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    lhs = lhs.checked_add(self.parse_muldiv()?)?;
                }
                Some(b'-') => {
                    self.pos += 1;
                    lhs = lhs.checked_sub(self.parse_muldiv()?)?;
                }
                _ => return Some(lhs),
            }
        }
    }

    fn parse_muldiv(&mut self) -> Option<i64> {
        let mut lhs = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    lhs = lhs.checked_mul(self.parse_unary()?)?;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let r = self.parse_unary()?;
                    if r == 0 {
                        return None;
                    }
                    lhs = lhs.checked_div(r)?;
                }
                Some(b'%') => {
                    self.pos += 1;
                    let r = self.parse_unary()?;
                    if r == 0 {
                        return None;
                    }
                    lhs = lhs.checked_rem(r)?;
                }
                _ => return Some(lhs),
            }
        }
    }

    fn parse_unary(&mut self) -> Option<i64> {
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                self.parse_unary()?.checked_neg()
            }
            Some(b'+') => {
                self.pos += 1;
                self.parse_unary()
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Option<i64> {
        match self.peek()? {
            b'(' => {
                self.pos += 1;
                let v = self.parse_addsub()?;
                self.skip_ws();
                if self.bytes.get(self.pos).copied() != Some(b')') {
                    return None;
                }
                self.pos += 1;
                Some(v)
            }
            b'0'..=b'9' => {
                let start = self.pos;
                while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                std::str::from_utf8(&self.bytes[start..self.pos])
                    .ok()
                    .and_then(|t| t.parse::<i64>().ok())
            }
            _ => None,
        }
    }
}

// ---------- Async path: evaluator with cooperative wait ----------
//
// `fire_dg` routes any body containing a `wait` statement here. The eval
// runs to completion as one tokio task, sleeping at each `wait` point
// via `tokio::time::sleep`. The semantic difference vs. the sync path:
// `Return(0)` is observable to the caller for sync bodies (cancels the
// host action); for async bodies, the caller has already returned `Done`
// before the script reaches a return, so cancellation isn't possible.
// This matches tbamud's effective behavior — a script that hits `wait`
// gives up its veto over the host action.

/// True if any statement in `block` is a `Wait` (recursively into nested
/// blocks). Used by `fire_dg` to pick the sync vs. async eval path.
pub fn block_contains_wait(block: &Block) -> bool {
    block.iter().any(stmt_contains_wait)
}

fn stmt_contains_wait(s: &Stmt) -> bool {
    match s {
        Stmt::Wait(_) => true,
        Stmt::If { then_body, elif_branches, else_body, .. } => {
            block_contains_wait(then_body)
                || elif_branches.iter().any(|(_, b)| block_contains_wait(b))
                || else_body.as_ref().is_some_and(|b| block_contains_wait(b))
        }
        Stmt::While { body, .. } => block_contains_wait(body),
        Stmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| block_contains_wait(&c.body))
                || default.as_ref().is_some_and(|b| block_contains_wait(b))
        }
        _ => false,
    }
}

type AsyncFlow<'a> = Pin<Box<dyn Future<Output = Result<Flow, EvalError>> + Send + 'a>>;

/// Async counterpart of [`eval_block`]. Sleeps at `wait` statements.
pub async fn eval_block_async(block: &Block, ctx: &EvalCtx, state: &mut State) -> Result<Outcome, EvalError> {
    match eval_stmts_async(block, ctx, state).await? {
        Flow::Continue => Ok(Outcome::Done),
        Flow::Halt => Ok(Outcome::Halt),
        Flow::Return(n) => Ok(Outcome::Return(n)),
        Flow::Break => Ok(Outcome::Done),
    }
}

fn eval_stmts_async<'a>(stmts: &'a [Stmt], ctx: &'a EvalCtx, state: &'a mut State) -> AsyncFlow<'a> {
    Box::pin(async move {
        for s in stmts {
            match eval_stmt_async(s, ctx, state).await? {
                Flow::Continue => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Continue)
    })
}

fn eval_stmt_async<'a>(stmt: &'a Stmt, ctx: &'a EvalCtx, state: &'a mut State) -> AsyncFlow<'a> {
    Box::pin(async move {
        match stmt {
            // Wait is the whole reason for this path.
            Stmt::Wait(arg) => {
                let interp = vars::substitute(arg, ctx, state);
                let secs = parse_wait_secs(&interp);
                if secs > 0 {
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                }
                Ok(Flow::Continue)
            }

            // Control-flow statements recurse into nested blocks via the
            // async path so any `wait` inside them is honored.
            Stmt::If { cond, then_body, elif_branches, else_body } => {
                if eval_cond(cond, ctx, state) {
                    return eval_stmts_async(then_body, ctx, state).await;
                }
                for (c, body) in elif_branches {
                    if eval_cond(c, ctx, state) {
                        return eval_stmts_async(body, ctx, state).await;
                    }
                }
                if let Some(body) = else_body {
                    return eval_stmts_async(body, ctx, state).await;
                }
                Ok(Flow::Continue)
            }

            Stmt::While { cond, body } => {
                let mut iters: u32 = 0;
                while eval_cond(cond, ctx, state) {
                    if iters >= LOOP_CAP {
                        tracing::warn!(
                            "DG while loop hit LOOP_CAP={} on '{}'", LOOP_CAP, ctx.self_name
                        );
                        break;
                    }
                    match eval_stmts_async(body, ctx, state).await? {
                        Flow::Continue => {}
                        Flow::Break => break,
                        other => return Ok(other),
                    }
                    iters += 1;
                }
                Ok(Flow::Continue)
            }

            Stmt::Switch { value, cases, default } => {
                let head = vars::substitute(value, ctx, state);
                let head_trim = head.trim().to_string();
                let mut matched: Option<usize> = None;
                for (i, c) in cases.iter().enumerate() {
                    if c.patterns.iter().any(|p| {
                        let pv = vars::substitute(p, ctx, state);
                        pv.trim() == head_trim
                    }) {
                        matched = Some(i);
                        break;
                    }
                }
                if let Some(start) = matched {
                    for c in &cases[start..] {
                        match eval_stmts_async(&c.body, ctx, state).await? {
                            Flow::Continue => {}
                            Flow::Break => return Ok(Flow::Continue),
                            other => return Ok(other),
                        }
                        if !c.fall_through {
                            return Ok(Flow::Continue);
                        }
                    }
                    if let Some(d) = default {
                        return eval_stmts_async(d, ctx, state).await;
                    }
                } else if let Some(d) = default {
                    return eval_stmts_async(d, ctx, state).await;
                }
                Ok(Flow::Continue)
            }

            // All other statements are exactly the sync version — no
            // awaits inside. Delegate to the sync `eval_stmt` to avoid
            // duplicating logic.
            _ => eval_stmt(stmt, ctx, state),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_locals() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn eval_bool_simple() {
        // No EvalCtx needed for the pure-string boolean evaluator.
        assert!(eval_bool("1 == 1"));
        assert!(!eval_bool("1 == 2"));
        assert!(eval_bool("1 < 2"));
        assert!(eval_bool("hello == hello"));
        assert!(!eval_bool("hello == world"));
        assert!(eval_bool("1 != 2"));
        assert!(eval_bool("1 == 1 && 2 == 2"));
        assert!(!eval_bool("1 == 1 && 2 == 3"));
        assert!(eval_bool("1 == 1 || 2 == 3"));
        assert!(eval_bool("!(1 == 2)"));
        assert!(eval_bool("(1 == 1)"));
    }

    #[test]
    fn eval_bool_keyword_match() {
        // `/=` is case-insensitive equality OR prefix match.
        assert!(eval_bool("pan /= pan"));
        assert!(eval_bool("pan /= PAN"));
        assert!(eval_bool("panning /= pan")); // prefix match
        assert!(!eval_bool("foo /= bar"));
    }

    #[test]
    fn eval_bool_substring() {
        assert!(eval_bool("hello world =~ world"));
        assert!(!eval_bool("hello =~ world"));
        assert!(eval_bool("hello !~ world"));
    }

    #[test]
    fn eval_bool_compound() {
        // From 7.trg #700:  if %cmd% /= pan && %arg% /= gold
        // After substitution: "pan /= pan && gold /= gold"
        assert!(eval_bool("pan /= pan && gold /= gold"));
        assert!(!eval_bool("pan /= pan && silver /= gold"));
    }

    #[test]
    fn eval_expr_arithmetic() {
        assert_eq!(eval_expr("1 + 1"), "2");
        assert_eq!(eval_expr("10 - 3"), "7");
        assert_eq!(eval_expr("4 * 5"), "20");
        assert_eq!(eval_expr("10 / 3"), "3");
        assert_eq!(eval_expr("10 % 3"), "1");
        // No arithmetic — pass through.
        assert_eq!(eval_expr("hello"), "hello");
        assert_eq!(eval_expr("42"), "42");
    }

    #[test]
    fn eval_expr_parens_and_precedence() {
        // Phase 9a: real arithmetic engine.
        assert_eq!(eval_expr("1 + 2 * 3"), "7");
        assert_eq!(eval_expr("(1 + 2) * 3"), "9");
        assert_eq!(eval_expr("((10 - 4) * 2) + 1"), "13");
        assert_eq!(eval_expr("100 - (((5 + 2) - 10) * 10)"), "130");
        assert_eq!(eval_expr("-1 + 2"), "1");
        assert_eq!(eval_expr("(2 * 3) + (4 * 5)"), "26");
        // Division/mod by zero falls back to raw input.
        assert_eq!(eval_expr("1 / 0"), "1 / 0");
        // Malformed — fallback.
        assert_eq!(eval_expr("(1 + 2"), "(1 + 2");
    }

    #[test]
    fn substitute_locals() {
        // No-ctx substitute test — needs a fake ctx.
        // Instead we test that `resolve` returns local vars correctly via
        // a HashMap.
        let mut locals = empty_locals();
        locals.insert("foo".to_string(), "42".to_string());
        // We can't easily build EvalCtx here without setting up the world,
        // so this test just covers the local-only branch via the parser
        // check in vars itself (covered there).
        let _ = locals;
    }
}

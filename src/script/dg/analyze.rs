//! Static analyzer for DG Scripts trigger bodies.
//!
//! Imports surface unmappable trigger flag letters as Info warnings already.
//! What they *can't* tell you is whether the body itself uses commands,
//! variable accessors, or `eval` expressions that the runtime doesn't yet
//! support — those silent-no-op at fire time. This module walks a parsed
//! AST and flags each unsupported call site, so the import report can
//! say "trigger #5201 uses `%actor.fighting%` (unsupported)" instead of
//! the player having to play through every mob and notice a panning trigger
//! does nothing.
//!
//! Hooked into [`crate::import::mapping::map_dg_triggers`]; one Info
//! warning per trigger summarising distinct issues.
//!
//! Conservative on purpose: false positives waste a builder's time but
//! false negatives waste *runtime* debugging, so this errs toward flagging
//! anything not in the known-supported set.

use super::ast::{Block, Stmt, SwitchCase};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// Body failed to parse. Body-level issue; the runtime will silently
    /// drop the script at fire time.
    ParseError,
    /// Verb not in [`super::cmds::dispatch`]'s known set.
    UnknownCommand,
    /// `%head.field%` where head is unknown, or field isn't supported on
    /// the typed accessor (`actor`, `victim`, `self`).
    UnknownVariable,
    /// Spell name in `dg_cast` doesn't match any [`crate::EffectType`],
    /// damage table, or heal table — the cast will silently no-op.
    UnknownDgCastSpell,
    /// `eval <var> <expr>` whose `<expr>` uses parens or multiple operators
    /// — the current evaluator only handles single-binary-op expressions.
    ComplexEvalExpression,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub kind: IssueKind,
    pub detail: String,
}

/// Parse + analyze. Returns one entry per distinct issue (deduplicated).
pub fn analyze(body: &str) -> Vec<Issue> {
    let block = match super::parser::parse(body) {
        Ok(b) => b,
        Err(e) => {
            return vec![Issue {
                kind: IssueKind::ParseError,
                detail: e.to_string(),
            }];
        }
    };
    let mut issues: Vec<Issue> = Vec::new();
    walk_block(&block, &mut issues);
    dedupe(&mut issues);
    issues
}

fn dedupe(issues: &mut Vec<Issue>) {
    let mut seen: std::collections::HashSet<(IssueKind, String)> = std::collections::HashSet::new();
    issues.retain(|i| seen.insert((i.kind.clone(), i.detail.clone())));
}

fn walk_block(block: &Block, out: &mut Vec<Issue>) {
    for s in block {
        walk_stmt(s, out);
    }
}

fn walk_stmt(stmt: &Stmt, out: &mut Vec<Issue>) {
    match stmt {
        Stmt::Comment(_) | Stmt::Halt | Stmt::Return(_) | Stmt::Break => {}

        Stmt::Cmd(line) => {
            check_cmd_line(line, out);
            check_interps(line, out);
        }

        Stmt::Set { value, .. } => check_interps(value, out),

        Stmt::Eval { expr, .. } => {
            check_interps(expr, out);
            // After interpolation, the expression might still have parens
            // or multiple ops. Conservative: flag if the *literal* (pre-
            // substitution) text shows complexity.
            if eval_expr_too_complex(expr) {
                out.push(Issue {
                    kind: IssueKind::ComplexEvalExpression,
                    detail: expr.trim().to_string(),
                });
            }
        }

        Stmt::If { cond, then_body, elif_branches, else_body } => {
            check_interps(cond, out);
            walk_block(then_body, out);
            for (c, b) in elif_branches {
                check_interps(c, out);
                walk_block(b, out);
            }
            if let Some(b) = else_body {
                walk_block(b, out);
            }
        }

        Stmt::While { cond, body } => {
            check_interps(cond, out);
            walk_block(body, out);
        }

        Stmt::Switch { value, cases, default } => {
            check_interps(value, out);
            for c in cases {
                walk_switch_case(c, out);
            }
            if let Some(d) = default {
                walk_block(d, out);
            }
        }

        Stmt::Wait(arg) => check_interps(arg, out),
        Stmt::Context(expr) => check_interps(expr, out),
        Stmt::Global(_) | Stmt::Unset(_) => {}
        Stmt::Nop(s) => check_interps(s, out),

        Stmt::Attach { trig_vnum, target } | Stmt::Detach { trig_vnum, target } => {
            check_interps(trig_vnum, out);
            check_interps(target, out);
        }

        Stmt::Remote { target, value, .. } => {
            check_interps(target, out);
            if let Some(v) = value {
                check_interps(v, out);
            }
        }
        Stmt::Rdelete { target, .. } => check_interps(target, out),
    }
}

fn walk_switch_case(c: &SwitchCase, out: &mut Vec<Issue>) {
    for p in &c.patterns {
        check_interps(p, out);
    }
    walk_block(&c.body, out);
}

// ---------- Command verb checks ----------

fn check_cmd_line(line: &str, out: &mut Vec<Issue>) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let verb_raw = line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('%')
        .trim_end_matches('%')
        .to_ascii_lowercase();
    if verb_raw.is_empty() {
        return;
    }
    if !is_known_command(&verb_raw) {
        out.push(Issue {
            kind: IssueKind::UnknownCommand,
            detail: verb_raw,
        });
        return;
    }
    // Extra check for dg_cast: parse the spell name and validate it.
    if verb_raw == "dg_cast" {
        if let Some(spell) = parse_dg_cast_spell(line) {
            // Skip runtime-resolved spell names (`dg_cast %spell% target`).
            if spell.contains('%') {
                return;
            }
            if !is_known_dg_cast_spell(&spell) {
                out.push(Issue {
                    kind: IssueKind::UnknownDgCastSpell,
                    detail: spell,
                });
            }
        }
    }
}

/// Extract the spell name from a `dg_cast` line. Recognises both the
/// quoted form (`dg_cast 'sleep' victim`) and the bare form
/// (`dg_cast sleep victim`).
fn parse_dg_cast_spell(line: &str) -> Option<String> {
    // Skip the verb token.
    let after_verb = line.split_once(char::is_whitespace).map(|(_, r)| r.trim_start())?;
    if let Some(rest) = after_verb.strip_prefix('\'') {
        return rest.find('\'').map(|i| rest[..i].to_string());
    }
    if let Some(rest) = after_verb.strip_prefix('"') {
        return rest.find('"').map(|i| rest[..i].to_string());
    }
    after_verb
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
}

fn is_known_dg_cast_spell(spell: &str) -> bool {
    if crate::EffectType::from_str(spell).is_some() {
        return true;
    }
    if super::cmds::dg_cast_damage_table_lookup(spell).is_some()
        || super::cmds::dg_cast_heal_table_lookup(spell).is_some()
        || super::cmds::dg_cast_remove_table_lookup(spell).is_some()
    {
        return true;
    }
    // Stock CircleMUD / tbamud spell names. Runtime silent-no-ops on
    // unmodeled buffs, but these are well-known canonical spells from the
    // stock spell list so we accept them at analysis time.
    let s = spell.trim().to_ascii_lowercase().replace(' ', "_");
    matches!(
        s.as_str(),
        "armor"
            | "bless"
            | "fly"
            | "silence"
            | "refresh"
            | "cure_blind"
            | "cure_poison"
            | "remove_poison"
            | "remove_curse"
            | "word_of_recall"
            | "animate_dead"
            | "detect_align"
            | "detect_alignment"
            | "detect_evil"
            | "detect_good"
            | "detect_poison"
            | "earthquake"
            | "summon"
            | "teleport"
            | "control_weather"
            | "create_food"
            | "create_water"
            | "enchant_weapon"
            | "identify"
            | "locate_object"
            | "infravision"
            | "strength"
            | "stone_skin"
            | "true_seeing"
            | "detect_invisibility"
            | "protection_from_evil"
            | "protection_from_good"
            | "sense_life"
            | "waterwalk"
            | "clot_minor"
            | "clot"
            | "minor_creation"
            | "magical_lock"
    )
}

/// All command verbs the runtime knows about, mirroring the dispatch arms
/// in [`super::cmds::dispatch`]. Keep in sync.
fn is_known_command(verb: &str) -> bool {
    if matches!(
        verb,
        // send/echo family
        "send" | "msend" | "osend" | "wsend"
        | "echo" | "mecho" | "oecho" | "wecho"
        | "recho" | "mrecho" | "orecho" | "wrecho"
        | "echoaround" | "mechoaround" | "oechoaround" | "wechoaround"
        | "echaround" | "echoround" | "echround"
        | "zoneecho" | "zecho" | "wzoneecho" | "mzoneecho" | "ozoneecho"
        // damage / teleport / purge / load
        | "damage" | "mdamage" | "odamage" | "wdamage"
        | "teleport" | "mteleport" | "oteleport" | "wteleport"
        | "purge" | "mpurge" | "opurge" | "wpurge"
        | "load" | "mload" | "oload" | "wload"
        | "log" | "mlog" | "olog" | "wlog"
        // dg_cast / dg_affect / force
        | "dg_cast" | "dg_affect"
        | "force" | "mforce" | "oforce" | "wforce"
        // mob memory + pursuit
        | "mremember" | "mforget" | "mhunt"
        // at-room dispatch
        | "at" | "mat" | "oat" | "wat"
        // door
        | "mdoor" | "odoor" | "wdoor" | "door"
        // Phase 8 — timer / transform
        | "otimer" | "mtimer" | "wtimer" | "timer"
        | "transform" | "mtransform" | "otransform"
    ) {
        return true;
    }
    // Mob world-command dispatch (Phase 5c): say/emote/give/kill/socials/etc.
    // The analyzer can't tell statically whether self is a mob, so accept
    // these across the board — they no-op in obj/room context, which is
    // tbamud's behavior anyway.
    super::mob_cmd::known_verbs().contains(&verb)
}

// ---------- Variable interpolation checks ----------

fn check_interps(s: &str, out: &mut Vec<Issue>) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        // Find closing % at paren depth 0 (mirrors the runtime
        // substitution rule so `%a.b(-%c%)%` is a single outer interp,
        // not `%a.b(-%`).
        let Some(j) = scan_interp_end(bytes, i + 1) else {
            break;
        };
        let inner = &s[i + 1..j];
        if !inner.is_empty() {
            // Nested case: also walk the inner so a malformed inner
            // doesn't escape the analyzer.
            if inner.as_bytes().contains(&b'%') {
                check_interps(inner, out);
            }
            check_one_interp(inner, out);
        }
        i = j + 1;
    }
}

fn scan_interp_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut j = start;
    while j < bytes.len() {
        match bytes[j] {
            b'\n' => return None,
            b'(' => depth += 1,
            b')' if depth > 0 => depth -= 1,
            b'%' if depth == 0 => return Some(j),
            _ => {}
        }
        j += 1;
    }
    None
}

fn check_one_interp(inner: &str, out: &mut Vec<Issue>) {
    // Split into head + optional field.
    let (head, field) = match inner.find('.') {
        Some(i) => (&inner[..i], Some(&inner[i + 1..])),
        None => (inner, None),
    };

    // Remote entity-var reference: `%<uuid>.<field>%` is always OK
    // (we don't know what's stored on the entity).
    if uuid::Uuid::parse_str(head).is_ok() {
        return;
    }

    // Call form: `%head.field(args)%` — actor/victim/self routes to a
    // mutator (gold/hitp/move/exp) or reader (varexists/has_item/eq);
    // arg/speech routes to text-field (contains); findmob/findobj
    // routes to vnum lookup. Bare-name heads (locals/globals) route
    // to text-field as well — tbamud lets `set s foo bar` then
    // `if %s.contains(foo)%` work. Don't flag any of those.
    if let Some(f) = field {
        if split_field_call(f).is_some() {
            // Constrained-surface heads still get flagged on unknown
            // call-form (cmd/random — both have a tightly defined set).
            if matches!(head, "cmd" | "random") {
                out.push(Issue {
                    kind: IssueKind::UnknownVariable,
                    detail: format!("%{}.{}%", head, f),
                });
            }
            return;
        }
    }

    // Chained-room form: `%head.room.field%` (Phase 6b). Recognise on
    // actor/victim/self. Unknown room fields fall through to room
    // dg_vars at runtime (Phase 8a), so don't flag them — only flag if
    // the head itself isn't a recognised actor head.
    if let Some(f) = field {
        if f.strip_prefix("room.").is_some() {
            let head_ok = matches!(head, "actor" | "victim" | "self");
            if !head_ok {
                out.push(Issue {
                    kind: IssueKind::UnknownVariable,
                    detail: format!("%{}.{}%", head, f),
                });
            }
            return;
        }
    }

    match head {
        "actor" | "victim" | "self" => {
            // Phase 8a: unknown fields are treated as per-entity dg_var
            // reads (tbamud's `remote <var> %actor.id%` pattern). Don't
            // flag — the runtime returns empty for missing keys, which
            // is the correct shape.
            let _ = field;
        }
        "random" => {
            // `%random.N%`, `%random.char%`, `%random.dir%` are supported
            // forms. Bare `%random%` returns a big random int. Anything
            // else flags.
            if let Some(f) = field {
                let known = matches!(f, "char" | "dir") || f.parse::<u32>().is_ok();
                if !known {
                    out.push(Issue {
                        kind: IssueKind::UnknownVariable,
                        detail: format!("%random.{}%", f),
                    });
                }
            }
        }
        "arg" | "speech" => {
            // Phase 8c relaxed: arg/speech accept text-field accessors
            // (car/cdr/strlen/contains) AND coerce to actor on a non-text
            // field (`%arg.id%`, `%arg.heshe%`, …). Don't flag.
            let _ = field;
        }
        "findmob" | "findobj" => {
            // Phase 8d: `%findmob.<vnum>%` / `%findmob.<vnum>(<altvnum>)%`.
            // Field shape is a bare integer or `<n>(<m>)`. Anything else
            // is shape-OK to the runtime (returns empty), so don't flag.
            let _ = field;
        }
        "cmd" => {
            // `%cmd%` and `%cmd.mudcommand%` are supported.
            if let Some(f) = field {
                if f != "mudcommand" {
                    out.push(Issue {
                        kind: IssueKind::UnknownVariable,
                        detail: format!("%cmd.{}%", f),
                    });
                }
            }
        }
        _ => {
            // Bare-name lookup against locals/globals/context. Always OK
            // shape-wise — the runtime treats unknown names as empty strings,
            // which is the correct behavior. (We have no way to know
            // statically what locals will be set.)
        }
    }
}

/// Parse `field(args)` into `(name, args)` if shape matches; else None.
fn split_field_call(field: &str) -> Option<(&str, &str)> {
    let open = field.find('(')?;
    let close = field.rfind(')')?;
    if close <= open {
        return None;
    }
    Some((&field[..open], &field[open + 1..close]))
}

// ---------- eval expression complexity check ----------

/// Returns true if the expression cannot be evaluated by `eval_expr`
/// once interpolation has been resolved. Replaces each `%...%` interp
/// with placeholder integer `1` and tries to parse the rest as
/// arithmetic. If parsing fails AND the expression looks arithmetic
/// (contains an operator or parens), flag it.
///
/// Bails (returns false) on nested-interpolation `%%...%%` shapes — those
/// resolve at runtime via Phase 7b nested interp scanning, and we can't
/// statically tell what shape they'll take.
fn eval_expr_too_complex(expr: &str) -> bool {
    if expr.contains("%%") {
        return false;
    }
    let stripped = strip_interps_for_arith(expr);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return false;
    }
    let has_op = trimmed
        .bytes()
        .any(|b| matches!(b, b'+' | b'*' | b'/' | b'%' | b'(' | b')'))
        || (trimmed.len() > 1 && trimmed[1..].contains('-'));
    if !has_op {
        return false;
    }
    super::eval::parse_arith(trimmed).is_none()
}

/// Replace `%...%` interpolation spans with the placeholder integer `1`
/// (1 dodges the div/mod-by-zero corner-case in our static check while
/// still being a valid number). Paren depth is tracked so a `%` inside
/// `%foo(%bar%)%` doesn't terminate the outer interp prematurely.
fn strip_interps_for_arith(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let bytes = expr.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let mut depth: i32 = 0;
            let mut j = i + 1;
            while j < bytes.len() {
                let c = bytes[j];
                if c == b'(' {
                    depth += 1;
                } else if c == b')' {
                    depth -= 1;
                } else if c == b'%' && depth == 0 {
                    break;
                }
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'%' {
                out.push('1');
                i = j + 1;
                continue;
            } else {
                out.push('%');
                i += 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

// ---------- Reporting helper ----------

/// Render a list of issues as a single short human-readable string. Used
/// by the importer to build one Info warning per trigger summarising all
/// distinct issues found.
pub fn summarize(issues: &[Issue]) -> String {
    let mut groups: std::collections::BTreeMap<&'static str, Vec<String>> = Default::default();
    for i in issues {
        let label = match i.kind {
            IssueKind::ParseError => "parse",
            IssueKind::UnknownCommand => "unknown cmds",
            IssueKind::UnknownVariable => "unknown vars",
            IssueKind::UnknownDgCastSpell => "unknown dg_cast spells",
            IssueKind::ComplexEvalExpression => "complex eval",
        };
        groups.entry(label).or_default().push(i.detail.clone());
    }
    let mut parts: Vec<String> = Vec::new();
    for (label, mut details) in groups {
        details.sort();
        details.dedup();
        // Cap the per-group list so a body with 50 issues doesn't blow
        // up the warning text.
        let preview: Vec<String> = details.iter().take(4).cloned().collect();
        let suffix = if details.len() > 4 {
            format!(", +{} more", details.len() - 4)
        } else {
            String::new()
        };
        parts.push(format!("{}: {}{}", label, preview.join(", "), suffix));
    }
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_unknown_command() {
        let issues = analyze("nonexistent_cmd whatever\nhalt");
        assert!(
            issues.iter().any(|i| i.kind == IssueKind::UnknownCommand && i.detail == "nonexistent_cmd"),
            "expected UnknownCommand for 'nonexistent_cmd', got {:?}", issues
        );
    }

    #[test]
    fn known_commands_pass() {
        // mecho is registered. msend, mload, dg_cast (with known spell) too.
        let issues = analyze(
            "mecho hello world\nmsend %actor% hi\nmload mob 1234\ndg_cast 'sleep' victim",
        );
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownCommand),
            "no UnknownCommand expected, got {:?}", issues
        );
    }

    #[test]
    fn unknown_actor_field_treated_as_dg_var_read() {
        // Phase 8a: tbamud's `remote zn118_blindquest %actor.id%` writes
        // builder-defined keys into per-actor dg_vars; reading them back
        // via `%actor.<key>%` is shape-OK. The analyzer no longer flags
        // unknown actor fields — they fall through to dg_vars at runtime.
        let issues = analyze("if %actor.zn118_blindquest% == 1\n  halt\nend");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected (dg_var read), got {:?}", issues
        );
    }

    #[test]
    fn known_actor_fields_pass() {
        let issues = analyze("if %actor.level% < 10\n  msend %actor% hello\nend");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}", issues
        );
    }

    #[test]
    fn flags_unknown_dg_cast_spell() {
        let issues = analyze("dg_cast 'foobazblort' victim");
        assert!(
            issues.iter().any(|i| i.kind == IssueKind::UnknownDgCastSpell && i.detail == "foobazblort"),
            "expected UnknownDgCastSpell, got {:?}", issues
        );
    }

    #[test]
    fn known_dg_cast_spell_passes() {
        // 'fireball' is in the damage table. 'sleep' is a known EffectType.
        let issues = analyze("dg_cast 'fireball' victim\ndg_cast 'sleep' victim");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownDgCastSpell),
            "expected no UnknownDgCastSpell, got {:?}", issues
        );
    }

    #[test]
    fn parenthesized_eval_now_supported() {
        // Phase 9a: parens parse cleanly via the recursive-descent eval.
        let issues = analyze("eval x (1 + 2)");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::ComplexEvalExpression),
            "no ComplexEvalExpression expected for parens, got {:?}", issues
        );
    }

    #[test]
    fn multi_op_eval_now_supported() {
        // Phase 9a: multi-op precedence handled.
        let issues = analyze("eval x 1 + 2 * 3");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::ComplexEvalExpression),
            "no ComplexEvalExpression expected for multi-op, got {:?}", issues
        );
    }

    #[test]
    fn simple_eval_passes() {
        let issues = analyze("eval x %i% + 1");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::ComplexEvalExpression),
            "expected no ComplexEvalExpression for simple eval, got {:?}", issues
        );
    }

    #[test]
    fn eval_with_interp_and_parens_passes() {
        // Real stock pattern: `eval x ((10 - (%foo% / 10)) * 2) + %random.101%`
        let issues = analyze("eval x ((10 - (%foo% / 10)) * 2) + %random.101%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::ComplexEvalExpression),
            "no ComplexEvalExpression expected, got {:?}", issues
        );
    }

    #[test]
    fn malformed_eval_still_flags() {
        // Unbalanced paren — runtime cannot parse this.
        let issues = analyze("eval x ((1 + 2)");
        assert!(
            issues.iter().any(|i| i.kind == IssueKind::ComplexEvalExpression),
            "expected ComplexEvalExpression for malformed expr, got {:?}", issues
        );
    }

    #[test]
    fn dedupe_collapses_repeats() {
        // Two trigger lines both call an unknown command — analyzer should
        // collapse them to one issue rather than emitting one per call site.
        let issues = analyze(
            "foobarcmd 1\nfoobarcmd 2",
        );
        let foobar_count = issues
            .iter()
            .filter(|i| i.kind == IssueKind::UnknownCommand && i.detail == "foobarcmd")
            .count();
        assert_eq!(foobar_count, 1, "expected dedupe to collapse, got {:?}", issues);
    }

    #[test]
    fn summarize_groups_by_kind() {
        let issues = vec![
            Issue { kind: IssueKind::UnknownCommand, detail: "foo".into() },
            Issue { kind: IssueKind::UnknownCommand, detail: "bar".into() },
            Issue { kind: IssueKind::UnknownVariable, detail: "%self.zip%".into() },
        ];
        let s = summarize(&issues);
        assert!(s.contains("unknown cmds: bar, foo"), "groups + alpha sort, got {}", s);
        assert!(s.contains("unknown vars: %self.zip%"), "got {}", s);
    }

    #[test]
    fn known_field_call_passes() {
        // gold/hitp/move on actor/victim/self are recognised mutating accessors.
        let issues = analyze("nop %actor.gold(-50)%\nnop %self.hitp(10)%\nnop %victim.move(-5)%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn unknown_field_call_silent_on_actor() {
        // Phase 8a: unknown call-form on actor/victim/self is shape-OK at
        // analyzer time — runtime returns empty for unknown call accessors.
        // Bare-name heads route to text-field semantics so they pass too.
        let issues = analyze("nop %actor.foobar(1)%\nnop %s.contains(foo)%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected (silent on actor + bare-name heads), got {:?}",
            issues
        );
        // Constrained-surface heads still flag unknown call-form.
        let issues = analyze("nop %cmd.foobar(1)%");
        assert!(
            issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "expected UnknownVariable on cmd head, got {:?}",
            issues
        );
    }

    #[test]
    fn random_char_and_dir_pass() {
        let issues = analyze("nop %random.char%\nnop %random.dir%\nnop %random.5%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn random_unknown_field_flagged() {
        let issues = analyze("nop %random.foo%");
        assert!(
            issues
                .iter()
                .any(|i| i.kind == IssueKind::UnknownVariable && i.detail.contains("foo")),
            "expected UnknownVariable for random.foo, got {:?}",
            issues
        );
    }

    #[test]
    fn chained_room_field_passes() {
        let issues = analyze(
            "if %self.room.vnum% == 3001\n  msend %actor% home\nend\nnop %actor.room.name%",
        );
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn chained_room_unknown_inner_field_silent() {
        // Phase 8a: unknown room fields fall through to room dg_vars at
        // runtime — no longer flagged by the analyzer.
        let issues = analyze("nop %self.room.foobar%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected (dg_var fall-through), got {:?}",
            issues
        );
    }

    #[test]
    fn zoneecho_known_command() {
        let issues = analyze("zoneecho The sun rises.\nzecho The wind howls.");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownCommand),
            "no UnknownCommand expected, got {:?}",
            issues
        );
    }

    #[test]
    fn wear_wield_remove_known_commands() {
        let issues = analyze("wear shield\nwield sword\nremove shield\nquaff potion\nconsider rat");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownCommand),
            "no UnknownCommand expected, got {:?}",
            issues
        );
    }

    #[test]
    fn varexists_and_has_item_call_pass() {
        let issues = analyze(
            "if %actor.varexists(quest_done)% == 1\n  msend %actor% hi\nend\nnop %actor.has_item(3001)%",
        );
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn align_and_maxhitp_pass() {
        let issues = analyze("if %actor.align% < -350\n  halt\nend\nnop %actor.maxhitp%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn arg_car_cdr_pass() {
        let issues = analyze("set first %arg.car%\nset rest %arg.cdr%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn nested_interp_inside_parens_does_not_terminate_outer() {
        // %actor.gold(-%random.50%)% should be a single outer interp.
        // Without paren-aware scan, it would be flagged as `actor.gold(-`.
        let issues = analyze("nop %actor.gold(-%random.50%)%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn inventory_and_eq_known() {
        let issues = analyze("nop %actor.inventory%\nnop %actor.eq(wield)%\nnop %self.contents%");
        assert!(
            !issues.iter().any(|i| i.kind == IssueKind::UnknownVariable),
            "no UnknownVariable expected, got {:?}",
            issues
        );
    }

    #[test]
    fn parse_error_is_a_single_issue() {
        // Unmatched if → parse error → single ParseError issue.
        let issues = analyze("if %x% == 1\n  halt\n");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::ParseError);
    }
}

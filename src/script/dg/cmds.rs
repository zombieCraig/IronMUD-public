//! DG Scripts command dispatch — translates DG verbs into IronMUD's
//! existing world API. The interpreter calls [`dispatch`] with a fully
//! interpolated command line; we tokenize and route.
//!
//! The handler set is the Phase 1 subset documented in [`super`]:
//! - send/echo family (`%send%`/`msend`/`osend`/`wsend`,
//!   `%echo%`/`mecho`/`oecho`/`wecho`, `%echoaround%`/`mechoaround`)
//! - `%damage%` / `mdamage` / `odamage` / `wdamage`
//! - `%teleport%` / `mteleport` / `oteleport` / `wteleport`
//! - `%purge%` / `mpurge` / `opurge` / `wpurge`
//! - `%load%` / `mload` / `oload` / `wload`
//!
//! Unknown commands log a debug warning and return `Ok(())` — silent
//! no-op matches tbamud's behavior on undefined commands.

use uuid::Uuid;

use super::{ActorRef, EvalCtx, SelfKind};
use crate::types::{DgAttachKind, DgTriggerProto, ItemLocation, ItemTrigger, MobileTrigger, RoomTrigger};

/// Tokenize and dispatch `line` (already variable-substituted).
pub fn dispatch(line: &str, ctx: &EvalCtx) -> Result<(), String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }
    let (verb, rest) = split_verb(line);
    let v = strip_pct(&verb).to_ascii_lowercase();

    match v.as_str() {
        "send" | "msend" | "osend" | "wsend" => cmd_send(rest, ctx),

        "echo" | "mecho" | "oecho" | "wecho" | "recho" | "mrecho" | "orecho" | "wrecho" => {
            cmd_echo(rest, ctx, None)
        }

        "echoaround" | "mechoaround" | "oechoaround" | "wechoaround"
        // Common stock typos / alternate spellings — same semantics.
        | "echaround" | "echoround" | "echround" => {
            let (target, msg) = split_verb(rest);
            cmd_echo(msg, ctx, Some(target.as_str()))
        }

        // Zone-wide broadcast: msg lands in every room sharing self_room's
        // area. Stock tbamud uses this for sunrise/sunset/weather flavor.
        "zoneecho" | "zecho" | "wzoneecho" | "mzoneecho" | "ozoneecho" => {
            cmd_zoneecho(rest, ctx)
        }

        "damage" | "mdamage" | "odamage" | "wdamage" => cmd_damage(rest, ctx),

        "teleport" | "mteleport" | "oteleport" | "wteleport" => cmd_teleport(rest, ctx),

        "purge" | "mpurge" | "opurge" | "wpurge" => cmd_purge(rest, ctx),

        "load" | "mload" | "oload" | "wload" => cmd_load(rest, ctx),

        "log" | "mlog" | "olog" | "wlog" => {
            tracing::info!("[dg trigger:{}] {}", ctx.self_name, rest);
            Ok(())
        }

        // ---- Phase 3 ----
        "dg_cast" => cmd_dg_cast(rest, ctx),
        "dg_affect" => cmd_dg_affect(rest, ctx),
        "morality" | "dg_morality" => cmd_dg_morality(rest, ctx),
        "force" | "mforce" | "oforce" | "wforce" => cmd_force(rest, ctx),

        // ---- Phase 4 ----
        // mremember / mforget — mob memory of players. IronMUD's mob memory
        // is keyed by player name on `MobileData.remembered_players` (Vec).
        "mremember" => cmd_mremember(rest, ctx),
        "mforget" => cmd_mforget(rest, ctx),
        // mhunt — set the mob's pursuit target. Falls back to no-op if the
        // target can't be resolved or is a player (we hunt by char id).
        "mhunt" => cmd_mhunt(rest, ctx),
        // mat / oat / wat — execute the rest as a DG cmd line in another
        // room context. Phase 4 simplification: temporarily override
        // ctx.self_room and recurse.
        "at" | "mat" | "oat" | "wat" => cmd_at(rest, ctx),
        // mdoor / odoor / wdoor — open/close/lock/unlock doors.
        "mdoor" | "odoor" | "wdoor" | "door" => cmd_door(rest, ctx),

        // ---- Phase 8 ----
        // otimer N — set item decay timer. Stored on dg_vars["timer"]
        // (no struct field), readable via %self.timer%. Mtimer/wtimer
        // alias for parity with stock idiom.
        "otimer" | "mtimer" | "wtimer" | "timer" => cmd_otimer(rest, ctx),
        // transform/mtransform/otransform — swap a mob or item's
        // prototype to a different vnum while preserving identity.
        "transform" | "mtransform" | "otransform" => cmd_transform(rest, ctx),

        // `award_achievement <player> <key>` — grant a Manual-criterion
        // achievement to the named player. Engine-criterion keys are
        // rejected at the achievements layer (mirrors the Rhai-side
        // `award_achievement` gate).
        "award_achievement" => cmd_award_achievement(rest, ctx),

        _ => {
            // Mob world-command dispatch (Phase 5c): when self is a mob,
            // recognise verbs like say/emote/give/kill that stock tbamud
            // mob bodies issue directly.
            if super::mob_cmd::try_dispatch(&v, rest, ctx) {
                return Ok(());
            }
            tracing::debug!("DG cmd '{}' not yet implemented (script self={})", v, ctx.self_name);
            super::warn_builder(ctx, &format!("unknown command: {v}"));
            Ok(())
        }
    }
}

/// Split off the leading whitespace-delimited verb token. Returns
/// `(verb, rest)` where `rest` has leading whitespace trimmed.
fn split_verb(s: &str) -> (String, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (s[..i].to_string(), s[i..].trim_start()),
        None => (s.to_string(), ""),
    }
}

/// Strip a leading and/or trailing `%` from a verb (DG style: `%send%`).
fn strip_pct(s: &str) -> &str {
    let s = s.strip_prefix('%').unwrap_or(s);
    s.strip_suffix('%').unwrap_or(s)
}

/// Canonical list of DG command verbs (lowercased, `%`-stripped). The
/// editor's syntax highlighter and tab-completion source consume this
/// directly so they never drift from the dispatcher. Aliases / typo
/// spellings (`echaround`, `wmtimer`, etc.) are intentionally omitted
/// from the surfaced list — keep them in `is_known_dg_verb` only.
pub const COMMANDS: &[&str] = &[
    "send", "msend", "osend", "wsend",
    "echo", "mecho", "oecho", "wecho", "recho", "mrecho", "orecho", "wrecho",
    "echoaround", "mechoaround", "oechoaround", "wechoaround",
    "zoneecho", "zecho",
    "damage", "mdamage", "odamage", "wdamage",
    "teleport", "mteleport", "oteleport", "wteleport",
    "purge", "mpurge", "opurge", "wpurge",
    "load", "mload", "oload", "wload",
    "log", "mlog", "olog", "wlog",
    "dg_cast", "dg_affect",
    "morality", "dg_morality",
    "force", "mforce", "oforce", "wforce",
    "mremember", "mforget", "mhunt",
    "at", "mat", "oat", "wat",
    "mdoor", "odoor", "wdoor", "door",
    "otimer", "mtimer", "wtimer", "timer",
    "transform", "mtransform", "otransform",
    "award_achievement",
];

/// Return `true` when `verb` (already lowercased + `%`-stripped) is a
/// known DG command. Used by the `Stmt::Cmd` evaluator to preserve a
/// leading `%verb%` token through variable substitution — without this,
/// `substitute` resolves `%send%` as a bare-name lookup and the
/// command line becomes args-only, dispatching the player name as the
/// verb. Keep this in sync with the `dispatch` match below.
pub(super) fn is_known_dg_verb(verb: &str) -> bool {
    matches!(
        verb,
        "send" | "msend" | "osend" | "wsend"
        | "echo" | "mecho" | "oecho" | "wecho" | "recho" | "mrecho" | "orecho" | "wrecho"
        | "echoaround" | "mechoaround" | "oechoaround" | "wechoaround"
        | "echaround" | "echoround" | "echround"
        | "zoneecho" | "zecho" | "wzoneecho" | "mzoneecho" | "ozoneecho"
        | "damage" | "mdamage" | "odamage" | "wdamage"
        | "teleport" | "mteleport" | "oteleport" | "wteleport"
        | "purge" | "mpurge" | "opurge" | "wpurge"
        | "load" | "mload" | "oload" | "wload"
        | "log" | "mlog" | "olog" | "wlog"
        | "dg_cast" | "dg_affect"
        | "morality" | "dg_morality"
        | "force" | "mforce" | "oforce" | "wforce"
        | "mremember" | "mforget" | "mhunt"
        | "at" | "mat" | "oat" | "wat"
        | "mdoor" | "odoor" | "wdoor" | "door"
        | "otimer" | "mtimer" | "wtimer" | "timer"
        | "transform" | "mtransform" | "otransform"
        | "award_achievement"
    )
}

// ---------- Commands ----------

fn cmd_send(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (target_tok, msg) = split_verb(rest);
    if target_tok.is_empty() || msg.is_empty() {
        return Ok(());
    }
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        return Ok(());
    };
    if let ActorRef::Player { connection_id, .. } = actor {
        if !connection_id.is_empty() {
            let mut text = msg.to_string();
            if !text.ends_with('\n') {
                text.push('\n');
            }
            crate::send_client_message(&ctx.connections, connection_id, text);
        }
    }
    Ok(())
}

fn cmd_echo(msg: &str, ctx: &EvalCtx, exclude: Option<&str>) -> Result<(), String> {
    let Some(room_id) = ctx.self_room else {
        return Ok(());
    };
    let mut text = msg.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let exclude_name = exclude
        .and_then(|t| resolve_target(t, ctx))
        .and_then(|a| match a {
            ActorRef::Player { name, .. } => Some(name),
            ActorRef::Mob { .. } => None,
        });
    crate::broadcast_to_room(&ctx.connections, room_id, text, exclude_name.as_deref());
    Ok(())
}

/// `zoneecho <msg>` — broadcast `msg` to every room in the same area as
/// `self_room`. No-op when self has no room or the room has no area.
/// Players in any of those rooms see the line; no exclude support
/// (matches tbamud's `zoneecho` semantics).
fn cmd_zoneecho(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let mut text = rest.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let Some(room_id) = ctx.self_room else {
        return Ok(());
    };
    let Ok(Some(room)) = ctx.db.get_room_data(&room_id) else {
        return Ok(());
    };
    let Some(area_id) = room.area_id else {
        return Ok(());
    };
    let Ok(rooms) = ctx.db.get_rooms_in_area(&area_id) else {
        return Ok(());
    };
    for r in rooms {
        crate::broadcast_to_room(&ctx.connections, r.id, text.clone(), None);
    }
    Ok(())
}

fn cmd_damage(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (target_tok, amt_str) = split_verb(rest);
    let amount: i32 = amt_str.trim().parse().unwrap_or(0);
    if amount <= 0 || target_tok.is_empty() {
        return Ok(());
    }
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        return Ok(());
    };
    match actor {
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                ch.hp = (ch.hp - amount).max(0);
                let _ = ctx.db.save_character_data(ch);
            }
        }
        ActorRef::Mob { mobile_id, .. } => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&mobile_id) {
                mob.current_hp = (mob.current_hp - amount).max(0);
                let _ = ctx.db.save_mobile_data(mob);
            }
        }
    }
    Ok(())
}

fn cmd_teleport(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (target_tok, dest_tok) = split_verb(rest);
    if target_tok.is_empty() || dest_tok.is_empty() {
        return Ok(());
    }
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        return Ok(());
    };
    let dest_id = match Uuid::parse_str(dest_tok.trim()) {
        Ok(u) => u,
        Err(_) => match ctx.db.get_room_by_vnum(dest_tok.trim()) {
            Ok(Some(r)) => r.id,
            _ => {
                super::warn_builder(ctx, &format!("teleport: unknown room '{}'", dest_tok.trim()));
                return Ok(());
            }
        },
    };
    // F3 gate: teleport crosses area boundaries trivially. Authorize
    // against the destination room's area.
    let dest_area = ctx
        .db
        .get_room_data(&dest_id)
        .ok()
        .flatten()
        .and_then(|r| r.area_id);
    if !ctx.opcode_authorized("teleport", dest_area) {
        super::warn_builder(ctx, "teleport blocked: author lacks permission for destination area");
        return Ok(());
    }
    match actor {
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                ch.current_room_id = dest_id;
                let _ = ctx.db.save_character_data(ch);
            }
        }
        ActorRef::Mob { mobile_id, .. } => {
            let _ = ctx.db.move_mobile_to_room(&mobile_id, &dest_id);
        }
    }
    Ok(())
}

fn cmd_purge(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (target_tok, _) = split_verb(rest);
    if target_tok.is_empty() {
        // Self-purge — gate against the host's own area.
        if !ctx.opcode_authorized("purge", None) {
            super::warn_builder(ctx, "purge blocked: author lacks permission for host area");
            return Ok(());
        }
        match ctx.self_kind {
            SelfKind::Mob => {
                let _ = ctx.db.delete_mobile(&ctx.self_id);
            }
            SelfKind::Obj => {
                let _ = ctx.db.delete_item(&ctx.self_id);
            }
            SelfKind::Room => {}
        }
        return Ok(());
    }
    if let Some(ActorRef::Mob { mobile_id, .. }) = resolve_target(&target_tok, ctx) {
        // F3 gate: deleting a foreign mob crosses areas. Resolve the
        // target's area before authorizing.
        let target_area = ctx
            .db
            .get_mobile_data(&mobile_id)
            .ok()
            .flatten()
            .and_then(|m| m.area_id);
        if !ctx.opcode_authorized("purge", target_area) {
            super::warn_builder(ctx, "purge blocked: author lacks permission for target's area");
            return Ok(());
        }
        let _ = ctx.db.delete_mobile(&mobile_id);
        return Ok(());
    }
    // Item branch — resolve target as an item UUID. Used by `%purge%
    // %object.id%` from OnReceive triggers (wishing well, quest turn-in)
    // and any `purge <uuid>` opcode whose target is an item rather than
    // a mob. Keyword-based item lookup isn't supported here — `%object.id%`
    // is the intended idiom.
    if let Ok(uid) = Uuid::parse_str(target_tok.trim()) {
        if let Ok(Some(item)) = ctx.db.get_item_data(&uid) {
            let target_area = item.area_id;
            if !ctx.opcode_authorized("purge", target_area) {
                super::warn_builder(ctx, "purge blocked: author lacks permission for item's area");
                return Ok(());
            }
            let _ = ctx.db.delete_item(&uid);
        }
    }
    Ok(())
}

fn cmd_load(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    // Forms supported (Phase 1):
    //   load mob <vnum>          - spawn mobile in self's room
    //   load obj <vnum>          - spawn item in self's room
    //   load obj <vnum> <actor>  - spawn item into actor's inventory
    let mut tok = rest.split_whitespace();
    let kind = tok.next().unwrap_or("").to_ascii_lowercase();
    let vnum = tok.next().unwrap_or("").to_string();
    let dest = tok.next().unwrap_or("").to_string();
    if vnum.is_empty() {
        return Ok(());
    }
    let Some(room_id) = ctx.self_room else {
        return Ok(());
    };
    // F3 gate: load lands content in `room_id`. Authorize against
    // that room's area (which is the host's room for plain load /
    // host's location for items in inventory).
    let target_area = ctx
        .db
        .get_room_data(&room_id)
        .ok()
        .flatten()
        .and_then(|r| r.area_id);
    if !ctx.opcode_authorized("load", target_area) {
        super::warn_builder(ctx, "load blocked: author lacks permission for host area");
        return Ok(());
    }
    match kind.as_str() {
        "m" | "mob" | "mobile" => match ctx.db.spawn_mobile_from_prototype(&vnum) {
            Ok(Some(mut spawned)) => {
                spawned.current_room_id = Some(room_id);
                let _ = ctx.db.save_mobile_data(spawned);
            }
            _ => {
                super::warn_builder(
                    ctx,
                    &format!("load: unknown mob vnum '{vnum}' (or world-max cap hit)"),
                );
            }
        },
        "o" | "obj" | "object" | "item" => {
            let Ok(Some(mut spawned)) = ctx.db.spawn_item_from_prototype(&vnum) else {
                super::warn_builder(
                    ctx,
                    &format!("load: unknown obj vnum '{vnum}' (or world-max cap hit)"),
                );
                return Ok(());
            };
            if dest.is_empty() {
                spawned.location = ItemLocation::Room(room_id);
                let _ = ctx.db.save_item_data(spawned);
            } else {
                match resolve_target(&dest, ctx) {
                    Some(ActorRef::Player { name, .. }) if !name.is_empty() => {
                        spawned.location = ItemLocation::Inventory(name);
                        let _ = ctx.db.save_item_data(spawned);
                    }
                    Some(ActorRef::Mob { mobile_id, .. }) => {
                        spawned.location = ItemLocation::Inventory(mobile_id.to_string());
                        let _ = ctx.db.save_item_data(spawned);
                    }
                    _ => {
                        spawned.location = ItemLocation::Room(room_id);
                        let _ = ctx.db.save_item_data(spawned);
                    }
                }
            }
        }
        other => {
            super::warn_builder(ctx, &format!("load: unknown kind '{other}' (expected m/mob or o/obj)"));
        }
    }
    Ok(())
}

/// `dg_cast '<spell>' <target>` — apply the named spell to the target.
///
/// Stock tbamud `dg_cast` covers three spell shapes:
/// 1. Buffs/debuffs (`sleep`, `blindness`, `sanctuary`, …) — name resolves
///    via [`crate::EffectType::from_str`] and we land an [`crate::ActiveBuff`].
/// 2. Damage (`fireball`, `harm`, `lightning bolt`, `magic missile`) —
///    handled by [`dg_cast_damage_table`] (subtracts HP).
/// 3. Healing (`cure light`, `cure serious`, `cure critic`, `heal`) —
///    handled by [`dg_cast_heal_table`] (adds HP, capped at max_hp).
///
/// Anything outside those three buckets is a silent no-op (matches the
/// tbamud convention of failing-quietly on unknown spell names).
fn cmd_dg_cast(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let (spell, target_tok) = if let Some(rest2) = trimmed.strip_prefix('\'') {
        match rest2.find('\'') {
            Some(i) => (rest2[..i].to_string(), rest2[i + 1..].trim_start().to_string()),
            None => {
                super::warn_builder(ctx, "dg_cast: missing closing quote");
                return Ok(());
            }
        }
    } else if let Some(rest2) = trimmed.strip_prefix('"') {
        match rest2.find('"') {
            Some(i) => (rest2[..i].to_string(), rest2[i + 1..].trim_start().to_string()),
            None => {
                super::warn_builder(ctx, "dg_cast: missing closing quote");
                return Ok(());
            }
        }
    } else {
        let (a, b) = split_verb(trimmed);
        (a, b.to_string())
    };

    // Damage / heal pre-handlers — match before the EffectType fallback so
    // names like `heal` (not in EffectType) don't silently no-op.
    if let Some(dmg) = dg_cast_damage_table(&spell) {
        apply_dg_damage(&target_tok, dmg, ctx);
        return Ok(());
    }
    if let Some(heal) = dg_cast_heal_table(&spell) {
        apply_dg_heal(&target_tok, heal, ctx);
        return Ok(());
    }
    // cure_*/remove_* spells strip a specific buff rather than apply one.
    // Stock idiom: dg_cast 'cure blind' victim, dg_cast 'remove poison' actor.
    if let Some(removed) = dg_cast_remove_table(&spell) {
        remove_dg_effect(removed, &target_tok, ctx);
        return Ok(());
    }
    apply_dg_effect(&spell, &target_tok, 1, default_dg_cast_duration(), ctx)
}

/// Map a stock cure/remove spell name to the EffectType it strips.
fn dg_cast_remove_table(spell: &str) -> Option<crate::EffectType> {
    let s = spell.trim().to_ascii_lowercase().replace(' ', "_");
    use crate::EffectType;
    match s.as_str() {
        "cure_blind" | "cure_blindness" | "remove_blind" | "remove_blindness" => Some(EffectType::Blind),
        "cure_poison" | "remove_poison" | "neutralize_poison" => Some(EffectType::Poison),
        "remove_curse" | "cure_curse" => Some(EffectType::Curse),
        "remove_sleep" | "wake" => Some(EffectType::Sleep),
        _ => None,
    }
}

/// Public alias for the static analyzer.
pub(crate) fn dg_cast_remove_table_lookup(spell: &str) -> Option<crate::EffectType> {
    dg_cast_remove_table(spell)
}

fn remove_dg_effect(effect: crate::EffectType, target_tok: &str, ctx: &EvalCtx) {
    let Some(actor) = resolve_target(target_tok, ctx) else {
        return;
    };
    match actor {
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                let before = ch.active_buffs.len();
                ch.active_buffs.retain(|b| b.effect_type != effect);
                if ch.active_buffs.len() != before {
                    let _ = ctx.db.save_character_data(ch);
                }
            }
        }
        ActorRef::Mob { mobile_id, .. } => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&mobile_id) {
                let before = mob.active_buffs.len();
                mob.active_buffs.retain(|b| b.effect_type != effect);
                if mob.active_buffs.len() != before {
                    let _ = ctx.db.save_mobile_data(mob);
                }
            }
        }
    }
}

/// Public alias for the static analyzer to validate dg_cast spell names
/// without re-implementing the table.
pub(crate) fn dg_cast_damage_table_lookup(spell: &str) -> Option<i32> {
    dg_cast_damage_table(spell)
}

/// Public alias for the static analyzer.
pub(crate) fn dg_cast_heal_table_lookup(spell: &str) -> Option<i32> {
    dg_cast_heal_table(spell)
}

/// Damage in HP for stock dg_cast offensive spells. `None` means "not a
/// damage spell" — caller falls through to buff / heal handlers.
fn dg_cast_damage_table(spell: &str) -> Option<i32> {
    let s = spell.trim().to_ascii_lowercase().replace(' ', "_");
    match s.as_str() {
        "magic_missile" => Some(8),
        "burning_hands" => Some(12),
        "shocking_grasp" => Some(15),
        "chill_touch" => Some(10),
        "lightning_bolt" => Some(20),
        "color_spray" => Some(15),
        "fireball" => Some(30),
        "energy_drain" => Some(25),
        "call_lightning" => Some(35),
        "harm" => Some(50),
        "dispel_evil" => Some(20),
        "dispel_good" => Some(20),
        _ => None,
    }
}

/// HP healed by stock dg_cast healing spells.
fn dg_cast_heal_table(spell: &str) -> Option<i32> {
    let s = spell.trim().to_ascii_lowercase().replace(' ', "_");
    match s.as_str() {
        "cure_light" => Some(8),
        "cure_serious" => Some(15),
        "cure_critic" | "cure_critical" => Some(25),
        "heal" => Some(100),
        "group_heal" => Some(100),
        _ => None,
    }
}

fn apply_dg_damage(target_tok: &str, amount: i32, ctx: &EvalCtx) {
    if amount <= 0 {
        return;
    }
    let Some(actor) = resolve_target(target_tok, ctx) else {
        return;
    };
    match actor {
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                ch.hp = (ch.hp - amount).max(0);
                let _ = ctx.db.save_character_data(ch);
            }
        }
        ActorRef::Mob { mobile_id, .. } => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&mobile_id) {
                mob.current_hp = (mob.current_hp - amount).max(0);
                let _ = ctx.db.save_mobile_data(mob);
            }
        }
    }
}

fn apply_dg_heal(target_tok: &str, amount: i32, ctx: &EvalCtx) {
    if amount <= 0 {
        return;
    }
    let Some(actor) = resolve_target(target_tok, ctx) else {
        return;
    };
    match actor {
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                let cap = ch.max_hp;
                ch.hp = (ch.hp + amount).min(cap);
                let _ = ctx.db.save_character_data(ch);
            }
        }
        ActorRef::Mob { mobile_id, .. } => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&mobile_id) {
                let cap = mob.max_hp;
                mob.current_hp = (mob.current_hp + amount).min(cap);
                let _ = ctx.db.save_mobile_data(mob);
            }
        }
    }
}

/// `dg_affect <target> <effect> <magnitude> <duration>` — direct buff.
/// Magnitude and duration are optional; defaults are 1 and 60s.
fn cmd_dg_affect(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let mut tok = rest.split_whitespace();
    let target_tok = tok.next().unwrap_or("").to_string();
    let effect = tok.next().unwrap_or("").to_string();
    let magnitude: i32 = tok.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let duration: i32 = tok.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    if target_tok.is_empty() || effect.is_empty() {
        super::warn_builder(ctx, "dg_affect: needs target and effect");
        return Ok(());
    }
    apply_dg_effect(&effect, &target_tok, magnitude, duration, ctx)
}

fn default_dg_cast_duration() -> i32 {
    300
}

/// `morality <target> <delta>` — adjust a player's morality slider. Mob targets
/// are silently ignored (mobiles have no morality field). Clamps to
/// [MORALITY_MIN, MORALITY_MAX]; result is read back via `%actor.morality%`.
fn cmd_dg_morality(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let mut tok = rest.split_whitespace();
    let target_tok = tok.next().unwrap_or("").to_string();
    let delta: i32 = match tok.next().and_then(|s| s.parse().ok()) {
        Some(d) => d,
        None => {
            super::warn_builder(ctx, "morality: needs target and integer delta");
            return Ok(());
        }
    };
    if target_tok.is_empty() {
        super::warn_builder(ctx, "morality: needs target and integer delta");
        return Ok(());
    }
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        return Ok(());
    };
    match actor {
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                let new_val = (ch.morality as i64)
                    .saturating_add(delta as i64)
                    .clamp(
                        crate::morality::MORALITY_MIN as i64,
                        crate::morality::MORALITY_MAX as i64,
                    ) as i32;
                ch.morality = new_val;
                let _ = ctx.db.save_character_data(ch);
            }
        }
        ActorRef::Mob { .. } => {
            // Mobs don't carry morality in this slice.
            tracing::debug!("DG morality: mob target ignored (no morality field on mobs)");
        }
    }
    Ok(())
}

/// Apply an effect-name buff to a target token. Used by dg_cast / dg_affect.
fn apply_dg_effect(
    effect_name: &str,
    target_tok: &str,
    magnitude: i32,
    duration: i32,
    ctx: &EvalCtx,
) -> Result<(), String> {
    let effect = match crate::EffectType::from_str(effect_name) {
        Some(e) => e,
        None => {
            tracing::debug!(
                "DG: unknown effect '{}' on dg_cast/dg_affect (target={})",
                effect_name, target_tok
            );
            super::warn_builder(ctx, &format!("dg_cast/dg_affect: unknown effect '{effect_name}'"));
            return Ok(());
        }
    };
    let Some(actor) = resolve_target(target_tok, ctx) else {
        return Ok(());
    };
    let buff = crate::ActiveBuff {
        effect_type: effect,
        magnitude,
        remaining_secs: duration,
        source: ctx.self_name.clone(),
        damage_type: None,
        vs_effect: None,
    };
    match actor {
        ActorRef::Mob { mobile_id, .. } => {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&mobile_id) {
                if let Some(existing) = mob.active_buffs.iter_mut().find(|b| b.effect_type == effect) {
                    existing.magnitude = existing.magnitude.max(magnitude);
                    existing.remaining_secs = duration;
                    existing.source = ctx.self_name.clone();
                } else {
                    mob.active_buffs.push(buff);
                }
                let _ = ctx.db.save_mobile_data(mob);
            }
        }
        ActorRef::Player { name, .. } => {
            if let Ok(Some(mut ch)) = ctx.db.get_character_data(&name) {
                if let Some(existing) = ch.active_buffs.iter_mut().find(|b| b.effect_type == effect) {
                    existing.magnitude = existing.magnitude.max(magnitude);
                    existing.remaining_secs = duration;
                    existing.source = ctx.self_name.clone();
                } else {
                    ch.active_buffs.push(buff);
                }
                let _ = ctx.db.save_character_data(ch);
            }
        }
    }
    Ok(())
}

/// `[mow]force <target> <command>` — inject `<command>` into the target's
/// input stream. Players run it through the regular command dispatch.
/// Mob targets are no-op for now (mob command engine not yet exposed).
fn cmd_force(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (target_tok, cmdline) = split_verb(rest);
    if target_tok.is_empty() || cmdline.is_empty() {
        return Ok(());
    }
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        return Ok(());
    };
    if let ActorRef::Player { connection_id, name, .. } = actor {
        if connection_id.is_empty() {
            return Ok(());
        }
        // F3 gate: snapshot the target player's room area + admin bit
        // so we can authorize without holding the connections lock for
        // the full opcode_authorized call. opcode_authorized may call
        // back into the db, which doesn't deadlock with connections
        // but releasing early keeps the lock window tight.
        let (cid, target_area, target_is_admin) = {
            let Ok(cid) = uuid::Uuid::parse_str(&connection_id) else {
                return Ok(());
            };
            let Ok(conns) = ctx.connections.lock() else {
                return Ok(());
            };
            let Some(session) = conns.get(&cid) else {
                return Ok(());
            };
            let ch = session.character.as_ref();
            let is_admin = ch.is_some_and(|c| c.is_admin);
            let area = ch
                .map(|c| c.current_room_id)
                .and_then(|rid| ctx.db.get_room_data(&rid).ok().flatten())
                .and_then(|r| r.area_id);
            (cid, area, is_admin)
        };
        // Refuse to force-inject commands into admin sessions. DG
        // triggers are builder-authored; allowing them to puppeteer
        // admins is an escalation path.
        if target_is_admin {
            tracing::warn!(
                "[SECURITY] DG force blocked: trigger attempted to force admin '{}' to run '{}'",
                name,
                cmdline
            );
            super::warn_builder(ctx, &format!("force blocked: cannot force admin '{name}'"));
            return Ok(());
        }
        if !ctx.opcode_authorized("force", target_area) {
            super::warn_builder(ctx, "force blocked: author lacks permission for target's area");
            return Ok(());
        }
        if let Ok(conns) = ctx.connections.lock() {
            if let Some(session) = conns.get(&cid) {
                let _ = session
                    .input_sender
                    .try_send(crate::InputEvent::Line(cmdline.to_string()));
            }
        }
    }
    Ok(())
}

/// `mremember <player>` — add the named player to this mob's
/// `remembered_enemies` list. Self-bound: only operates if `%self%` is
/// a mob.
fn cmd_mremember(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    if ctx.self_kind != SelfKind::Mob {
        return Ok(());
    }
    let (target_tok, _) = split_verb(rest);
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        return Ok(());
    };
    let name = match actor {
        ActorRef::Player { name, .. } => name,
        ActorRef::Mob { .. } => return Ok(()),
    };
    if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&ctx.self_id) {
        if !mob.remembered_enemies.iter().any(|e| e.name.eq_ignore_ascii_case(&name)) {
            // Default 1-hour expiry — caller can override via dg_affect-style
            // direct manipulation if longer durations are needed.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            mob.remembered_enemies.push(crate::types::RememberedEnemy {
                name,
                expires_at_secs: now + 3600,
            });
            let _ = ctx.db.save_mobile_data(mob);
        }
    }
    Ok(())
}

/// `mforget <player>` — remove the named player from `remembered_enemies`.
fn cmd_mforget(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    if ctx.self_kind != SelfKind::Mob {
        return Ok(());
    }
    let (target_tok, _) = split_verb(rest);
    if target_tok.is_empty() {
        return Ok(());
    }
    if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&ctx.self_id) {
        let before = mob.remembered_enemies.len();
        mob.remembered_enemies
            .retain(|e| !e.name.eq_ignore_ascii_case(&target_tok));
        if mob.remembered_enemies.len() != before {
            let _ = ctx.db.save_mobile_data(mob);
        }
    }
    Ok(())
}

/// `mhunt <target>` — set the mob's pursuit target to the named player.
/// Resolved via the existing pursuit fields used by the cross-room
/// pursuit tick. Self-bound to mobs.
fn cmd_mhunt(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    if ctx.self_kind != SelfKind::Mob {
        return Ok(());
    }
    let (target_tok, _) = split_verb(rest);
    let Some(actor) = resolve_target(&target_tok, ctx) else {
        // Empty target clears pursuit.
        if target_tok.trim().is_empty() {
            if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&ctx.self_id) {
                mob.pursuit_target_name.clear();
                mob.pursuit_target_room = None;
                mob.pursuit_certain = false;
                let _ = ctx.db.save_mobile_data(mob);
            }
        }
        return Ok(());
    };
    let name = match actor {
        ActorRef::Player { name, .. } => name,
        ActorRef::Mob { .. } => return Ok(()),
    };
    let dest_room = ctx
        .db
        .get_character_data(&name)
        .ok()
        .flatten()
        .map(|c| c.current_room_id);
    if let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&ctx.self_id) {
        mob.pursuit_target_name = name;
        mob.pursuit_target_room = dest_room;
        mob.pursuit_certain = true;
        let _ = ctx.db.save_mobile_data(mob);
    }
    Ok(())
}

/// `mat <room> <cmdline>` — execute the rest as a DG cmd line bound to a
/// different room. We rebind `self_room` for the recursive dispatch and
/// run the inner command. The mob isn't physically moved — this matches
/// tbamud's behavior for `mat` (echoes/loads land in the named room).
fn cmd_at(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (room_tok, inner) = split_verb(rest);
    if room_tok.is_empty() || inner.is_empty() {
        return Ok(());
    }
    let dest_id = match Uuid::parse_str(room_tok.trim()) {
        Ok(u) => u,
        Err(_) => match ctx.db.get_room_by_vnum(room_tok.trim()) {
            Ok(Some(r)) => r.id,
            _ => {
                super::warn_builder(ctx, &format!("at: unknown room '{}'", room_tok.trim()));
                return Ok(());
            }
        },
    };
    // F3 gate: `at` rebinds self_room to a different area. Authorize
    // against the destination room's area before running the inner
    // command. Inner commands also re-authorize themselves, but this
    // catches the rebind itself (echo/load to a foreign room).
    let dest_area = ctx
        .db
        .get_room_data(&dest_id)
        .ok()
        .flatten()
        .and_then(|r| r.area_id);
    if !ctx.opcode_authorized("at", dest_area) {
        super::warn_builder(ctx, "at blocked: author lacks permission for destination area");
        return Ok(());
    }
    let mut sub = ctx.clone();
    sub.self_room = Some(dest_id);
    dispatch(inner, &sub)
}

/// `mdoor / odoor / wdoor <room> <dir> <field> <value>` — mutate a door's
/// state on the named room. Fields: `purge` (remove the door),
/// `description`, `flags` (open/closed/locked/unlocked/pickproof/normal).
fn cmd_door(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let mut tok = rest.split_whitespace();
    let room_tok = tok.next().unwrap_or("").to_string();
    let dir = tok.next().unwrap_or("").to_ascii_lowercase();
    let field = tok.next().unwrap_or("").to_ascii_lowercase();
    let value: String = tok.collect::<Vec<_>>().join(" ");
    if room_tok.is_empty() || dir.is_empty() || field.is_empty() {
        return Ok(());
    }
    let room_id = match Uuid::parse_str(&room_tok) {
        Ok(u) => u,
        Err(_) => match ctx.db.get_room_by_vnum(&room_tok) {
            Ok(Some(r)) => r.id,
            _ => {
                super::warn_builder(ctx, &format!("door: unknown room '{room_tok}'"));
                return Ok(());
            }
        },
    };
    if !matches!(field.as_str(), "purge" | "description" | "flags") {
        super::warn_builder(
            ctx,
            &format!("door: unknown field '{field}' (expected purge/description/flags)"),
        );
        return Ok(());
    }
    let _ = ctx.db.update_room(&room_id, |room| match field.as_str() {
        "purge" => {
            room.doors.remove(&dir);
        }
        "description" => {
            if let Some(d) = room.doors.get_mut(&dir) {
                d.description = if value.is_empty() { None } else { Some(value.clone()) };
            }
        }
        "flags" => {
            let want = value.to_ascii_lowercase();
            let entry = room.doors.entry(dir.clone()).or_default();
            for word in want.split(|c: char| c == ',' || c.is_whitespace()) {
                match word.trim() {
                    "open" => entry.is_closed = false,
                    "closed" | "close" => entry.is_closed = true,
                    "locked" | "lock" => {
                        entry.is_closed = true;
                        entry.is_locked = true;
                    }
                    "unlocked" | "unlock" => entry.is_locked = false,
                    "pickproof" => entry.pickproof = true,
                    "nopickproof" => entry.pickproof = false,
                    "normal" => {
                        entry.is_closed = false;
                        entry.is_locked = false;
                        entry.pickproof = false;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    });
    Ok(())
}

/// Resolve `target_tok` to a host UUID (mob, item, or room), then push a
/// new trigger built from prototype `vnum` onto its `triggers` list.
/// Used by both the DG `attach` statement and the builder
/// `trigger dg attach <vnum>` subcommand.
pub(crate) fn attach_trigger_proto(vnum: &str, target_tok: &str, ctx: &EvalCtx) {
    let proto = match ctx.db.get_dg_trigger_proto(vnum.trim()) {
        Ok(Some(p)) => p,
        _ => {
            tracing::debug!("DG: attach: unknown trigger proto vnum={}", vnum);
            super::warn_builder(ctx, &format!("attach: no trigger proto vnum '{}'", vnum.trim()));
            return;
        }
    };
    apply_attach(&proto, target_tok, ctx);
}

/// Reverse of [`attach_trigger_proto`]: remove every trigger whose
/// `dg_name` (or vnum-prefixed name) matches the prototype's name.
/// We don't track the original vnum on the trigger struct, so detach
/// matches by `dg_name`.
pub(crate) fn detach_trigger_proto(vnum: &str, target_tok: &str, ctx: &EvalCtx) {
    let proto = match ctx.db.get_dg_trigger_proto(vnum.trim()) {
        Ok(Some(p)) => p,
        _ => {
            super::warn_builder(ctx, &format!("detach: no trigger proto vnum '{}'", vnum.trim()));
            return;
        }
    };
    apply_detach(&proto, target_tok, ctx);
}

fn apply_attach(proto: &DgTriggerProto, target_tok: &str, ctx: &EvalCtx) {
    let target = resolve_host_uuid(target_tok, ctx);
    let Some(host) = target else { return };

    match (proto.attach_kind, host) {
        (DgAttachKind::Mob, HostRef::Mob(uid)) => {
            let trig_types = crate::import::engines::tba::trg_map::mobile_trigger_types(&proto.flags);
            if trig_types.is_empty() {
                return;
            }
            let _ = ctx.db.update_mobile(&uid, |mob| {
                for ttype in &trig_types {
                    mob.triggers.push(MobileTrigger {
                        trigger_type: *ttype,
                        script_name: String::new(),
                        enabled: true,
                        chance: proto.numeric_arg.clamp(1, 100),
                        args: arglist_to_args(&proto.arglist),
                        interval_secs: 60,
                        last_fired: 0,
                        dg_body: Some(proto.body.clone()),
                        dg_name: Some(proto.name.clone()),
                        authored_by: None,
                        elevated: false,
                        source_proto_vnum: Some(proto.vnum.clone()),
                    });
                }
            });
        }
        (DgAttachKind::Obj, HostRef::Obj(uid)) => {
            let trig_types = crate::import::engines::tba::trg_map::item_trigger_types(&proto.flags);
            if trig_types.is_empty() {
                return;
            }
            let _ = ctx.db.update_item(&uid, |item| {
                for ttype in &trig_types {
                    item.triggers.push(ItemTrigger {
                        trigger_type: *ttype,
                        script_name: String::new(),
                        enabled: true,
                        chance: proto.numeric_arg.clamp(1, 100),
                        args: arglist_to_args(&proto.arglist),
                        dg_body: Some(proto.body.clone()),
                        dg_name: Some(proto.name.clone()),
                        authored_by: None,
                        elevated: false,
                        source_proto_vnum: Some(proto.vnum.clone()),
                    });
                }
            });
        }
        (DgAttachKind::Room, HostRef::Room(uid)) => {
            let trig_types = crate::import::engines::tba::trg_map::room_trigger_types(&proto.flags);
            if trig_types.is_empty() {
                return;
            }
            let _ = ctx.db.update_room(&uid, |room| {
                for ttype in &trig_types {
                    room.triggers.push(RoomTrigger {
                        trigger_type: *ttype,
                        script_name: String::new(),
                        enabled: true,
                        interval_secs: 60,
                        last_fired: 0,
                        chance: proto.numeric_arg.clamp(1, 100),
                        args: arglist_to_args(&proto.arglist),
                        dg_body: Some(proto.body.clone()),
                        dg_name: Some(proto.name.clone()),
                        authored_by: None,
                        elevated: false,
                        source_proto_vnum: Some(proto.vnum.clone()),
                    });
                }
            });
        }
        _ => {
            tracing::debug!(
                "DG: attach: kind mismatch (proto={:?} target_tok={})",
                proto.attach_kind, target_tok
            );
        }
    }
}

fn apply_detach(proto: &DgTriggerProto, target_tok: &str, ctx: &EvalCtx) {
    let Some(host) = resolve_host_uuid(target_tok, ctx) else { return };
    // Match by source_proto_vnum first (set since Part 3); fall back to
    // dg_name for legacy un-tagged instances from earlier imports.
    let matches_proto = |source: Option<&str>, name: Option<&str>| -> bool {
        source == Some(&proto.vnum) || (source.is_none() && name == Some(&proto.name))
    };
    match (proto.attach_kind, host) {
        (DgAttachKind::Mob, HostRef::Mob(uid)) => {
            let _ = ctx.db.update_mobile(&uid, |mob| {
                mob.triggers
                    .retain(|t| !matches_proto(t.source_proto_vnum.as_deref(), t.dg_name.as_deref()));
            });
        }
        (DgAttachKind::Obj, HostRef::Obj(uid)) => {
            let _ = ctx.db.update_item(&uid, |item| {
                item.triggers
                    .retain(|t| !matches_proto(t.source_proto_vnum.as_deref(), t.dg_name.as_deref()));
            });
        }
        (DgAttachKind::Room, HostRef::Room(uid)) => {
            let _ = ctx.db.update_room(&uid, |room| {
                room.triggers
                    .retain(|t| !matches_proto(t.source_proto_vnum.as_deref(), t.dg_name.as_deref()));
            });
        }
        _ => {}
    }
}

/// Split a `.trg` arglist string into `args: Vec<String>` for the
/// resulting trigger. Stock `.trg` arglists are space-separated keyword
/// lists (e.g. `"pan gold"` for a COMMAND trigger that wants verb `pan`).
fn arglist_to_args(arglist: &str) -> Vec<String> {
    let s = arglist.trim();
    if s.is_empty() {
        Vec::new()
    } else {
        s.split_whitespace().map(|w| w.to_string()).collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum HostRef {
    Mob(Uuid),
    Obj(Uuid),
    Room(Uuid),
}

/// Resolve a token to a host entity ref. Tries: special tokens (`self`,
/// `actor`, `victim`, `here`/`room`), UUID parse + per-tree lookup,
/// then prototype-vnum fallback for mobs/items/rooms (matching the DG
/// convention of `attach <trig> <vnum>`).
fn resolve_host_uuid(tok: &str, ctx: &EvalCtx) -> Option<HostRef> {
    let t = tok.trim();
    if t.is_empty() {
        return None;
    }
    match t.to_ascii_lowercase().as_str() {
        "self" => match ctx.self_kind {
            SelfKind::Mob => return Some(HostRef::Mob(ctx.self_id)),
            SelfKind::Obj => return Some(HostRef::Obj(ctx.self_id)),
            SelfKind::Room => return Some(HostRef::Room(ctx.self_id)),
        },
        "here" | "room" => {
            if let Some(rid) = ctx.self_room {
                return Some(HostRef::Room(rid));
            }
        }
        "actor" | "victim" => {
            // Players have no DG host shape (no triggers attach to chars).
            return None;
        }
        _ => {}
    }
    if let Ok(uid) = Uuid::parse_str(t) {
        if ctx.db.get_mobile_data(&uid).ok().flatten().is_some() {
            return Some(HostRef::Mob(uid));
        }
        if ctx.db.get_item_data(&uid).ok().flatten().is_some() {
            return Some(HostRef::Obj(uid));
        }
        if ctx.db.get_room_data(&uid).ok().flatten().is_some() {
            return Some(HostRef::Room(uid));
        }
        return None;
    }
    // Prototype vnum fallback.
    if let Ok(Some(m)) = ctx.db.get_mobile_by_vnum(t) {
        return Some(HostRef::Mob(m.id));
    }
    if let Ok(Some(i)) = ctx.db.get_item_by_vnum(t) {
        return Some(HostRef::Obj(i.id));
    }
    if let Ok(Some(r)) = ctx.db.get_room_by_vnum(t) {
        return Some(HostRef::Room(r.id));
    }
    None
}

/// `otimer N` — set the self-item's decay timer to `N`. Stored as a
/// dg_var so we don't need a struct field; readable via `%self.timer%`.
/// Phase 8e: stock-tbamud item-decay parity is an open follow-up; this
/// just persists the value so trigger bodies can read what they wrote.
fn cmd_otimer(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let n = rest.trim();
    if n.is_empty() {
        return Ok(());
    }
    if !matches!(ctx.self_kind, SelfKind::Obj) {
        return Ok(());
    }
    if let Ok(Some(mut item)) = ctx.db.get_item_data(&ctx.self_id) {
        item.dg_vars.insert("timer".to_string(), n.to_string());
        let _ = ctx.db.save_item_data(item);
    }
    Ok(())
}

/// `transform <vnum>` — swap a mob's prototype to a different vnum while
/// preserving identity (id, current_room_id, hp, dg_vars). Stock tbamud
/// uses this for shapeshift / morph mechanics. `otransform` is the same
/// idea for items.
///
/// Implementation: load the prototype for `vnum`, copy its descriptive
/// fields onto the live entity, save. Falls through silently if the vnum
/// has no prototype.
fn cmd_transform(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let vnum = rest.split_whitespace().next().unwrap_or("").to_string();
    if vnum.is_empty() {
        return Ok(());
    }
    match ctx.self_kind {
        SelfKind::Mob => {
            let (Ok(Some(mut live)), Ok(Some(proto))) = (
                ctx.db.get_mobile_data(&ctx.self_id),
                ctx.db.spawn_mobile_from_prototype(&vnum),
            ) else {
                return Ok(());
            };
            // Stock tbamud transform rewrites short_desc / long_desc /
            // description / keywords; HP and combat survive.
            live.vnum = vnum;
            live.name = proto.name;
            live.short_desc = proto.short_desc;
            live.long_desc = proto.long_desc;
            live.keywords = proto.keywords;
            live.flags = proto.flags;
            // Tear down the temporary prototype clone we created.
            let _ = ctx.db.delete_mobile(&proto.id);
            let _ = ctx.db.save_mobile_data(live);
        }
        SelfKind::Obj => {
            let (Ok(Some(mut live)), Ok(Some(proto))) = (
                ctx.db.get_item_data(&ctx.self_id),
                ctx.db.spawn_item_from_prototype(&vnum),
            ) else {
                return Ok(());
            };
            live.vnum = Some(vnum);
            live.name = proto.name;
            live.short_desc = proto.short_desc;
            live.long_desc = proto.long_desc;
            live.keywords = proto.keywords;
            live.flags = proto.flags;
            live.item_type = proto.item_type;
            let _ = ctx.db.delete_item(&proto.id);
            let _ = ctx.db.save_item_data(live);
        }
        SelfKind::Room => {}
    }
    Ok(())
}

/// `award_achievement <player> <key>` — grant the named manual-criterion
/// achievement to the player. `<player>` accepts the same forms as other
/// DG target tokens (`actor`, a name, or a UUID). Silently no-ops when:
/// the player can't be resolved, the key is unknown, the criterion isn't
/// `Manual`, the player already has it, or the achievement system is
/// disabled. See `crate::script::achievements::award_manual_via_db`.
fn cmd_award_achievement(rest: &str, ctx: &EvalCtx) -> Result<(), String> {
    let (player_tok, key_rest) = split_verb(rest);
    let key = key_rest.split_whitespace().next().unwrap_or("");
    if player_tok.is_empty() || key.is_empty() {
        return Ok(());
    }
    let Some(actor) = resolve_target(&player_tok, ctx) else {
        return Ok(());
    };
    let player_name = match actor {
        ActorRef::Player { name, .. } if !name.is_empty() => name,
        _ => return Ok(()),
    };
    crate::script::achievements::award_manual_via_db(&ctx.db, &ctx.connections, &player_name, key);
    Ok(())
}

/// Resolve a DG target token (a name, keyword, or UUID string) to an
/// [`ActorRef`]. Recognised inputs:
/// - exact UUID → look up character/mob
/// - exact actor/victim name (`%actor%` was already substituted to the name)
/// - special tokens `actor`, `victim`
fn resolve_target(tok: &str, ctx: &EvalCtx) -> Option<ActorRef> {
    let tok = tok.trim();
    if tok.is_empty() {
        return None;
    }
    if let Ok(uid) = Uuid::parse_str(tok) {
        if let Ok(Some(mob)) = ctx.db.get_mobile_data(&uid) {
            return Some(ActorRef::Mob { mobile_id: uid, name: mob.name });
        }
        // Match against bound actor/victim by char_id.
        for cand in [&ctx.actor, &ctx.victim].into_iter().flatten() {
            if let ActorRef::Player { char_id, .. } = cand {
                if *char_id == uid {
                    return Some(cand.clone());
                }
            }
        }
        return None;
    }
    match tok {
        "actor" => return ctx.actor.clone(),
        "victim" => return ctx.victim.clone(),
        _ => {}
    }
    // Name match against bound actor/victim, then fall back to looking up
    // a player character by name.
    for cand in [&ctx.actor, &ctx.victim].into_iter().flatten() {
        if cand.name().eq_ignore_ascii_case(tok) {
            return Some(cand.clone());
        }
    }
    if let Ok(Some(ch)) = ctx.db.get_character_data(tok) {
        return Some(ActorRef::Player {
            connection_id: String::new(),
            char_id: Uuid::nil(),
            name: ch.name,
        });
    }
    None
}

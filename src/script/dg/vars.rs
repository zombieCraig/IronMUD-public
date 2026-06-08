//! Variable substitution + field access for DG Scripts.
//!
//! Substitutes `%var%` and `%var.field%` expressions in a string against
//! the [`super::EvalCtx`]. Unknown variables expand to empty string
//! (matches tbamud's permissive behavior on missing fields).
//!
//! Phase 1 supports the common set of fields:
//! - `name`, `level`, `hitp`, `maxhp`, `gold`, `move`, `vnum`, `is_pc`,
//!   `room`, `fighting`
//! - `str/dex/con/int/wis/cha` ability scores
//! - `class`, `race` (PCs only)

use super::eval::State;
use super::{ActorRef, EvalCtx, SelfKind};

/// Walk `s`, replacing every `%expr%` with its resolved value.
///
/// Recognises `%name%`, `%name.field%`, `%random.N%`, and the call form
/// `%head.field(args)%` — including args that themselves embed `%var%`
/// interpolations (e.g. `%actor.gold(-%random.50%)%`). Inner `%`s are
/// allowed only when balanced parens contain them; the closing `%` is
/// the next `%` at paren depth 0.
///
/// Inner content is recursively substituted before the outer expression
/// is resolved, so `gold(-%random.50%)` first becomes `gold(-25)` and is
/// then dispatched as a call form.
pub fn substitute(s: &str, ctx: &EvalCtx, state: &State) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            let ch_len = utf8_char_len(bytes[i]);
            out.push_str(&s[i..i + ch_len]);
            i += ch_len;
            continue;
        }
        // Scan for the matching closing `%` at paren-depth 0. Newline
        // inside an unterminated interpolation cancels the scan and we
        // emit the literal `%`.
        match scan_interp_end(bytes, i + 1) {
            Some(j) => {
                let raw = &s[i + 1..j];
                if raw.is_empty() {
                    // `%%` → literal `%`.
                    out.push('%');
                } else if raw.as_bytes().contains(&b'%') {
                    // Nested form: recursively substitute the inner
                    // %vars% first, then resolve the outer.
                    let inner = substitute(raw, ctx, state);
                    out.push_str(&resolve(&inner, ctx, state));
                } else {
                    out.push_str(&resolve(raw, ctx, state));
                }
                i = j + 1;
            }
            None => {
                out.push('%');
                i += 1;
            }
        }
    }
    out
}

/// Return the index of the closing `%` for an interpolation that opened
/// at `start - 1`. Tracks `(` / `)` depth: inner `%` chars are skipped
/// when depth > 0, so `%a.b(-%c%)%` matches the outer `%` after `)`.
/// Returns None for unterminated (newline or end-of-string).
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

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xC0 {
        1 // invalid leading byte; advance to avoid infinite loop
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Resolve a single `%var%` or `%var.field%` expression (the part between
/// the percents, no surrounding `%`s).
///
/// Resolution order for a bare `%name%`:
///   1. `state.locals[name]` if set and `name` is not a global
///   2. world `dg_globals[name]` (if `name` was declared `global`,
///      or if the bare name has no local binding)
///   3. (when `state.context` is set) the context entity's `dg_vars[name]`
///   4. empty string
///
/// For `%name.field%`: if `name` parses as a UUID, treat it as a remote
/// entity-var lookup (`dg_vars[field]` on that entity). Otherwise fall
/// back to the typed accessors (`actor.level`, `self.vnum`, …).
pub fn resolve(expr: &str, ctx: &EvalCtx, state: &State) -> String {
    let (head, field) = match expr.find('.') {
        Some(i) => (&expr[..i], Some(&expr[i + 1..])),
        None => (expr, None),
    };

    // Remote entity-var lookup: `%<uuid>.<field>%`.
    if field.is_some() {
        if let Ok(uid) = uuid::Uuid::parse_str(head) {
            if let Some(f) = field {
                return remote_entity_var(ctx, &uid, f);
            }
        }
    }

    // Chained-area form: `%head.area.people%` / `.players` / `.mobs`, plus
    // optional filter call form `%head.area.mobs(rat)%`. Resolves head's
    // room, then that room's area, then iterates every room in the area.
    // Checked BEFORE the generic call-form block so the filtered variant
    // (`area.mobs(rat)`) isn't swallowed by `parse_field_call`. Supported on
    // actor / victim / self.
    if let Some(field_str) = field {
        if let Some(inner) = field_str.strip_prefix("area.") {
            let room_id = match head {
                "actor" => actor_room_id(ctx.actor.as_ref(), ctx),
                "victim" => actor_room_id(ctx.victim.as_ref(), ctx),
                "self" => ctx.self_room,
                _ => None,
            };
            let Some(rid) = room_id else {
                return String::new();
            };
            let Some(area_id) = ctx
                .db
                .get_room_data(&rid)
                .ok()
                .flatten()
                .and_then(|r| r.area_id)
            else {
                return String::new();
            };
            let (name, arg) = parse_field_call(inner).unwrap_or((inner, ""));
            let filter = (!arg.is_empty()).then_some(arg);
            return match name {
                "people" => list_area_people(ctx, &area_id, AreaKind::Both, filter),
                "players" | "pcs" => list_area_people(ctx, &area_id, AreaKind::Players, filter),
                "mobs" => list_area_people(ctx, &area_id, AreaKind::Mobs, filter),
                _ => String::new(),
            };
        }
    }

    // Call form: `%head.field(args)%` — splits to (fn_name, args). The
    // dispatcher routes to either a mutator (gold/hitp/move/exp — Phase 5d)
    // or a reader (varexists/has_item/eq — Phase 7). Recognised on
    // actor / victim / self only.
    if let Some(field_str) = field {
        if let Some((fn_name, args)) = parse_field_call(field_str) {
            return match head {
                "actor" => apply_actor_call(ctx.actor.as_ref(), fn_name, args, ctx, state),
                "victim" => apply_actor_call(ctx.victim.as_ref(), fn_name, args, ctx, state),
                "self" => apply_self_call(fn_name, args, ctx, state),
                _ => String::new(),
            };
        }
    }

    // Chained-room form: `%head.room.field%` resolves head's room then
    // fetches a field on that RoomData. Supported on actor/victim/self.
    if let Some(field_str) = field {
        if let Some(inner) = field_str.strip_prefix("room.") {
            let room_id = match head {
                "actor" => actor_room_id(ctx.actor.as_ref(), ctx),
                "victim" => actor_room_id(ctx.victim.as_ref(), ctx),
                "self" => ctx.self_room,
                _ => None,
            };
            let Some(rid) = room_id else {
                return String::new();
            };
            // `%head.room.people%` — comma-joined list of occupants. Don't
            // fall through to `room_field`, which has no people accessor.
            if inner == "people" {
                return room_people(ctx, &rid);
            }
            return ctx
                .db
                .get_room_data(&rid)
                .ok()
                .flatten()
                .map(|r| {
                    if let Some(v) = try_room_field(&r, inner) {
                        v
                    } else {
                        r.dg_vars.get(inner).cloned().unwrap_or_default()
                    }
                })
                .unwrap_or_default();
        }
    }

    // Phase 8d: `%findmob.<vnum>%` / `%findobj.<vnum>%` — first live
    // (non-prototype) instance of that vnum world-wide. The (altvnum)
    // call form falls back to alt when primary not found.
    if matches!(head, "findmob" | "findobj") {
        if let Some(field_str) = field {
            return resolve_find(head, field_str, ctx);
        }
        return String::new();
    }

    match head {
        "actor" => resolve_actor_field(ctx.actor.as_ref(), field, ctx),
        "victim" => resolve_actor_field(ctx.victim.as_ref(), field, ctx),
        "self" => resolve_self_field(ctx, field),
        // Fire-site object binding. Resolves the item referenced by
        // `context_vars["item_id"]` (set by give.rhai on OnReceive, and
        // any future fire site that hands the trigger an item context).
        // `%object%` → item name; `%object.field%` routes through
        // try_item_field (name/vnum/cost/type/shortdesc/...). Falls
        // through to the item's `dg_vars` when the field is unknown,
        // matching the actor/self per-entity fallback in Phase 8a.
        "object" => resolve_object_field(ctx, field),
        "arg" | "speech" => {
            // Phase 8c: arg-as-actor. Try entity coercion when the field
            // isn't a known text-field accessor; fall back to text-field.
            if let Some(f) = field {
                if !matches!(f, "car" | "cdr" | "strlen") && parse_field_call(f).is_none() {
                    if let Some(v) = arg_as_actor_field(&ctx.arg, f, ctx) {
                        return v;
                    }
                }
            }
            resolve_text_field(&ctx.arg, field)
        }
        "cmd" => match field {
            None => ctx.cmd.clone(),
            // tbamud's `%cmd.mudcommand%` returns the canonical (un-
            // abbreviated) verb. Falls back to `cmd` itself when the
            // canonical wasn't populated (only OnCommand fires set it).
            Some("mudcommand") => {
                if ctx.cmd_canonical.is_empty() {
                    ctx.cmd.clone()
                } else {
                    ctx.cmd_canonical.clone()
                }
            }
            _ => String::new(),
        },
        "random" => resolve_random(field, ctx),
        "time" => resolve_time(field, ctx),
        "weather" => resolve_weather(field, ctx),
        "season" => resolve_season(ctx),
        "sunlight" => resolve_sunlight(ctx),
        // Bare-name lookup: locals → durable globals → context entity vars.
        // When a field is present (`%s.contains(foo)%`, `%s.car%`), apply
        // the text-field reader on the resolved bare-name value. Stock
        // tbamud uses this idiom for string locals (`set s "foo bar"`
        // then `if %s.contains(foo)%`).
        _ => {
            let value = resolve_bare_name(head, ctx, state);
            if field.is_none() {
                value
            } else {
                resolve_text_field(&value, field)
            }
        }
    }
}

/// Phase 8d: `%findmob.<vnum>%` and `%findobj.<vnum>%`. The vnum may be
/// a bare integer or `<vnum>(<altvnum>)` for fallback. Returns the entity
/// id (mobile UUID / item UUID) of the first live, non-prototype instance,
/// or empty if none exist.
fn resolve_find(head: &str, field: &str, ctx: &EvalCtx) -> String {
    let (primary, alt) = if let Some((v, a)) = parse_field_call(field) {
        (v.trim().to_string(), Some(a.trim().to_string()))
    } else {
        (field.trim().to_string(), None)
    };
    let lookup = |vnum: &str| -> Option<String> {
        if vnum.is_empty() {
            return None;
        }
        match head {
            "findmob" => ctx.db.list_all_mobiles().ok().and_then(|mobs| {
                mobs.into_iter()
                    .find(|m| {
                        !m.is_prototype
                            && m.current_room_id.is_some()
                            && m.vnum == vnum
                    })
                    .map(|m| m.id.to_string())
            }),
            "findobj" => ctx.db.list_all_items().ok().and_then(|items| {
                items
                    .into_iter()
                    .find(|i| {
                        !i.is_prototype && i.vnum.as_deref().map(|v| v == vnum).unwrap_or(false)
                    })
                    .map(|i| i.id.to_string())
            }),
            _ => None,
        }
    };
    if let Some(s) = lookup(&primary) {
        return s;
    }
    if let Some(av) = alt.as_deref() {
        if let Some(s) = lookup(av) {
            return s;
        }
    }
    String::new()
}

/// Phase 8c: arg-as-actor. When `%arg.field%` is queried with a non-text
/// field (anything other than `car`/`cdr`/`strlen`/`contains(...)`), resolve
/// arg as an entity name in the self-room and route the field through the
/// character_field/mobile_field readers. Falls back to text-field handling
/// if no in-room entity matches.
fn arg_as_actor_field(text: &str, field: &str, ctx: &EvalCtx) -> Option<String> {
    let needle = text.split_whitespace().next()?.to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    let room_id = ctx.self_room?;
    if let Ok(chars) = ctx.db.list_all_characters() {
        for c in chars {
            if c.current_room_id == room_id && c.name.to_ascii_lowercase() == needle {
                if matches!(field, "name") {
                    return Some(c.name);
                }
                if matches!(field, "is_pc") {
                    return Some("1".to_string());
                }
                // PCs are keyed by name in IronMUD; no stable Uuid on
                // CharacterData. `%arg.id%` falls through to the name as
                // a best-effort handle.
                if matches!(field, "id") {
                    return Some(c.name);
                }
                if let Some(v) = try_character_field(&c, field) {
                    return Some(v);
                }
                return Some(c.dg_vars.get(field).cloned().unwrap_or_default());
            }
        }
    }
    if let Ok(mobs) = ctx.db.get_mobiles_in_room(&room_id) {
        for m in mobs {
            let lname = m.name.to_ascii_lowercase();
            let kw_match = m
                .keywords
                .iter()
                .any(|k| k.to_ascii_lowercase() == needle || k.to_ascii_lowercase().starts_with(&needle));
            if lname == needle || lname.contains(&needle) || kw_match {
                if matches!(field, "name") {
                    return Some(m.name);
                }
                if matches!(field, "id") {
                    return Some(m.id.to_string());
                }
                if matches!(field, "is_pc") {
                    return Some("0".to_string());
                }
                if let Some(v) = try_mobile_field(&m, field) {
                    return Some(v);
                }
                return Some(m.dg_vars.get(field).cloned().unwrap_or_default());
            }
        }
    }
    None
}

fn resolve_bare_name(name: &str, ctx: &EvalCtx, state: &State) -> String {
    // If declared global, durable lookup wins (and there may not even
    // be a local entry yet — e.g. a sibling trigger set the global).
    if state.globals.contains(name) {
        if let Some(v) = lookup_durable(ctx, state, name) {
            return v;
        }
    }
    if let Some(v) = state.locals.get(name) {
        return v.clone();
    }
    // Fire-site event bindings (`amount` on OnBribe, `item_name`/`giver`
    // on OnReceive, `killer` on OnDeath, etc.). Sits between locals and
    // durable lookup so a stale dg_var can't shadow the fire-time value.
    if let Some(v) = ctx.context_vars.get(name) {
        return v.clone();
    }
    if let Some(v) = lookup_durable(ctx, state, name) {
        return v;
    }
    String::new()
}

fn lookup_durable(ctx: &EvalCtx, state: &State, name: &str) -> Option<String> {
    // Context entity's dg_vars take precedence when context is set.
    if let Some(scope) = state.context.as_ref() {
        if let Some(v) = scope_var_opt(ctx, scope, name) {
            return Some(v);
        }
    }
    // World globals.
    ctx.db.get_dg_global(name).ok().flatten()
}

/// Read `field` from the scope's `dg_vars`. Mirrors
/// `eval::set_entity_var` — handles UUID-keyed mob/item/room and
/// name-keyed PCs.
fn scope_var_opt(ctx: &EvalCtx, scope: &super::eval::ScopeRef, field: &str) -> Option<String> {
    match scope {
        super::eval::ScopeRef::Uuid(uid) => remote_entity_var_opt(ctx, uid, field),
        super::eval::ScopeRef::Player(pname) => ctx
            .db
            .get_character_data(pname)
            .ok()
            .flatten()
            .and_then(|ch| ch.dg_vars.get(field).cloned()),
    }
}

fn remote_entity_var(ctx: &EvalCtx, uid: &uuid::Uuid, field: &str) -> String {
    remote_entity_var_opt(ctx, uid, field).unwrap_or_default()
}

fn remote_entity_var_opt(ctx: &EvalCtx, uid: &uuid::Uuid, field: &str) -> Option<String> {
    if let Ok(Some(mob)) = ctx.db.get_mobile_data(uid) {
        return mob.dg_vars.get(field).cloned();
    }
    if let Ok(Some(item)) = ctx.db.get_item_data(uid) {
        return item.dg_vars.get(field).cloned();
    }
    if let Ok(Some(room)) = ctx.db.get_room_data(uid) {
        return room.dg_vars.get(field).cloned();
    }
    None
}

/// Look up the current room id for a bound actor. Used by chained
/// `%actor.room.field%` resolution. Players query db; mobs cache it on
/// `MobileData.current_room_id`.
fn actor_room_id(actor: Option<&ActorRef>, ctx: &EvalCtx) -> Option<uuid::Uuid> {
    let actor = actor?;
    match actor {
        ActorRef::Player { name, .. } => ctx
            .db
            .get_character_data(name)
            .ok()
            .flatten()
            .map(|c| c.current_room_id),
        ActorRef::Mob { mobile_id, .. } => ctx
            .db
            .get_mobile_data(mobile_id)
            .ok()
            .flatten()
            .and_then(|m| m.current_room_id),
    }
}

fn resolve_actor_field(actor: Option<&ActorRef>, field: Option<&str>, ctx: &EvalCtx) -> String {
    let Some(actor) = actor else {
        return String::new();
    };
    let Some(field) = field else {
        // Bare `%actor%` — DG returns the entity UID. We approximate with
        // the entity's stable identifier (uuid for mobs, name for players —
        // names are the canonical PC handle in IronMUD).
        return match actor {
            ActorRef::Player { name, .. } => name.clone(),
            ActorRef::Mob { mobile_id, .. } => mobile_id.to_string(),
        };
    };
    // Collection accessors that need db access: resolved here before
    // falling through to the typed struct readers.
    if let Some(s) = collection_actor_field(actor, field, ctx) {
        return s;
    }
    match actor {
        ActorRef::Player { name, .. } => {
            // Cheap fields first to avoid the db hit.
            match field {
                "name" => return name.clone(),
                "is_pc" => return "1".to_string(),
                // PCs are keyed by name in IronMUD (no character UUID).
                // Returning the name here makes `remote VAR %actor.id% V`
                // and `context %actor.id%` route through `parse_scope_ref`
                // to the character's `dg_vars`.
                "id" => return name.clone(),
                _ => {}
            }
            if let Ok(Some(ch)) = ctx.db.get_character_data(name) {
                // Phase 8a: typed fields win; unknown fields fall through to
                // per-character `dg_vars` (tbamud's remote/var pattern).
                if let Some(v) = try_character_field(&ch, field) {
                    return v;
                }
                ch.dg_vars.get(field).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        }
        ActorRef::Mob { mobile_id, name } => {
            match field {
                "name" => return name.clone(),
                "is_pc" => return "0".to_string(),
                "id" => return mobile_id.to_string(),
                _ => {}
            }
            if let Ok(Some(mob)) = ctx.db.get_mobile_data(mobile_id) {
                if let Some(v) = try_mobile_field(&mob, field) {
                    return v;
                }
                mob.dg_vars.get(field).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

/// Database-backed actor field accessors that don't fit the pure struct
/// readers in [`character_field`] / [`mobile_field`]. Returns Some(s) if
/// the field was handled (even if the value is empty), None to fall
/// through to the typed reader.
fn collection_actor_field(actor: &ActorRef, field: &str, ctx: &EvalCtx) -> Option<String> {
    match field {
        "inventory" => {
            let names: Vec<String> = match actor {
                ActorRef::Player { name, .. } => ctx
                    .db
                    .get_items_in_inventory(name)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| i.name)
                    .collect(),
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_items_in_mobile_inventory(mobile_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| i.name)
                    .collect(),
            };
            Some(names.join(", "))
        }
        "equipped" => {
            let names: Vec<String> = match actor {
                ActorRef::Player { name, .. } => ctx
                    .db
                    .get_equipped_items(name)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| i.name)
                    .collect(),
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_items_equipped_on_mobile(mobile_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| i.name)
                    .collect(),
            };
            Some(names.join(", "))
        }
        _ => None,
    }
}

fn resolve_self_field(ctx: &EvalCtx, field: Option<&str>) -> String {
    match (ctx.self_kind, field) {
        (_, None) => ctx.self_id.to_string(),
        (_, Some("id")) => ctx.self_id.to_string(),
        (_, Some("name")) => ctx.self_name.clone(),
        (_, Some("vnum")) => ctx.self_vnum.clone(),
        (_, Some("room")) => ctx.self_room.map(|r| r.to_string()).unwrap_or_default(),
        // `%self.area%` — the UUID of self's area (resolved via self's room),
        // parallel to `%self.room%`. Empty when self has no room/area.
        (_, Some("area")) => ctx
            .self_room
            .and_then(|rid| ctx.db.get_room_data(&rid).ok().flatten())
            .and_then(|r| r.area_id)
            .map(|a| a.to_string())
            .unwrap_or_default(),
        (SelfKind::Mob, Some("inventory")) => ctx
            .db
            .get_items_in_mobile_inventory(&ctx.self_id)
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.name)
            .collect::<Vec<_>>()
            .join(", "),
        (SelfKind::Mob, Some("equipped")) => ctx
            .db
            .get_items_equipped_on_mobile(&ctx.self_id)
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.name)
            .collect::<Vec<_>>()
            .join(", "),
        // Direction-as-field on a mob resolves the mob's current room's
        // exit in that direction. Pattern: `if %self.east%` checks for
        // an east exit from the room the mob stands in.
        (SelfKind::Mob, Some(f @ ("north" | "south" | "east" | "west" | "up" | "down"))) => {
            ctx.db
                .get_mobile_data(&ctx.self_id)
                .ok()
                .flatten()
                .and_then(|m| m.current_room_id)
                .and_then(|rid| ctx.db.get_room_data(&rid).ok().flatten())
                .map(|r| room_field(&r, f))
                .unwrap_or_default()
        }
        (SelfKind::Mob, Some("people")) => self_room_people(ctx),
        (SelfKind::Mob, Some("followers")) => self_followers(ctx),
        (SelfKind::Mob, Some(f)) => {
            if let Ok(Some(mob)) = ctx.db.get_mobile_data(&ctx.self_id) {
                if let Some(v) = try_mobile_field(&mob, f) {
                    return v;
                }
                mob.dg_vars.get(f).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        }
        // Item-self accessors: `carried_by` returns the holder's name when
        // the item is in someone's inventory; `worn_by` returns the wearer
        // when equipped. Both empty when the item is in a room/container.
        (SelfKind::Obj, Some("carried_by")) => self_item_owner(ctx, /*equipped=*/ false),
        (SelfKind::Obj, Some("worn_by")) => self_item_owner(ctx, /*equipped=*/ true),
        (SelfKind::Obj, Some("contents")) => self_item_contents(ctx),
        (SelfKind::Obj, Some(f)) => {
            if let Ok(Some(item)) = ctx.db.get_item_data(&ctx.self_id) {
                if let Some(v) = try_item_field(&item, f) {
                    return v;
                }
                item.dg_vars.get(f).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        }
        (SelfKind::Room, Some("people")) => room_people(ctx, &ctx.self_id),
        (SelfKind::Room, Some(f)) => {
            if let Ok(Some(room)) = ctx.db.get_room_data(&ctx.self_id) {
                if let Some(v) = try_room_field(&room, f) {
                    return v;
                }
                room.dg_vars.get(f).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

/// Comma-joined list of names of all people (mobs + players) in the room
/// the self mob currently stands in, excluding the mob itself. Used by
/// `%self.people%` as a poor-man's iteration.
fn self_room_people(ctx: &EvalCtx) -> String {
    let Some(rid) = ctx.self_room else {
        return String::new();
    };
    let exclude = Some(ctx.self_id);
    list_room_people(ctx, &rid, exclude)
}

/// `%self.room.people%` and `%self.people%` for a SelfKind::Room script.
fn room_people(ctx: &EvalCtx, room_id: &uuid::Uuid) -> String {
    list_room_people(ctx, room_id, None)
}

fn list_room_people(ctx: &EvalCtx, room_id: &uuid::Uuid, exclude: Option<uuid::Uuid>) -> String {
    let mut names: Vec<String> = Vec::new();
    if let Ok(mobs) = ctx.db.get_mobiles_in_room(room_id) {
        for m in mobs {
            if Some(m.id) == exclude {
                continue;
            }
            names.push(m.name);
        }
    }
    if let Ok(chars) = ctx.db.list_all_characters() {
        for c in chars {
            if c.current_room_id == *room_id {
                names.push(c.name);
            }
        }
    }
    names.join(", ")
}

/// Which occupants `list_area_people` collects for the `%head.area.*%` forms.
#[derive(Clone, Copy)]
enum AreaKind {
    /// `area.people` — both players and mobs.
    Both,
    /// `area.players` / `area.pcs` — players only.
    Players,
    /// `area.mobs` — mobs only.
    Mobs,
}

/// Returns true when `entity` (a mob or player) matches the optional filter.
/// Match is case-insensitive: name-contains OR any keyword starts-with OR
/// (mobs) vnum-equals. `None` filter matches everything. Mirrors the lookup
/// semantics of `find_mobile_by_keyword_anywhere`.
fn area_filter_matches(name: &str, keywords: &[String], vnum: Option<&str>, filter: &str) -> bool {
    let f = filter.to_ascii_lowercase();
    if name.to_ascii_lowercase().contains(&f) {
        return true;
    }
    if keywords.iter().any(|k| k.to_ascii_lowercase().starts_with(&f)) {
        return true;
    }
    if let Some(v) = vnum {
        if v.eq_ignore_ascii_case(filter) {
            return true;
        }
    }
    false
}

/// `%head.area.people%` / `.players` / `.mobs` — comma-joined names of the
/// occupants of every room in `area_id`. The optional `filter` narrows the
/// list (chiefly to tame area-wide mob counts), e.g. `%self.area.mobs(rat)%`.
/// Player rows come from the same source as `list_room_people` (DB chars by
/// `current_room_id`); mobs from `get_mobiles_in_room` per area room.
fn list_area_people(
    ctx: &EvalCtx,
    area_id: &uuid::Uuid,
    kind: AreaKind,
    filter: Option<&str>,
) -> String {
    let Ok(rooms) = ctx.db.get_rooms_in_area(area_id) else {
        return String::new();
    };
    let room_ids: std::collections::HashSet<uuid::Uuid> = rooms.iter().map(|r| r.id).collect();
    let mut names: Vec<String> = Vec::new();

    if matches!(kind, AreaKind::Both | AreaKind::Mobs) {
        for rid in &room_ids {
            if let Ok(mobs) = ctx.db.get_mobiles_in_room(rid) {
                for m in mobs {
                    if filter.is_none_or(|f| {
                        area_filter_matches(&m.name, &m.keywords, Some(&m.vnum), f)
                    }) {
                        names.push(m.name);
                    }
                }
            }
        }
    }

    if matches!(kind, AreaKind::Both | AreaKind::Players) {
        if let Ok(chars) = ctx.db.list_all_characters() {
            for c in chars {
                if room_ids.contains(&c.current_room_id)
                    && filter.is_none_or(|f| area_filter_matches(&c.name, &[], None, f))
                {
                    names.push(c.name);
                }
            }
        }
    }

    names.join(", ")
}

/// `%self.followers%` — comma-joined names of mobs whose `charm_master` is
/// the self mob's name, or charmed-mobs in the same room. Approximates
/// tbamud's followers list.
fn self_followers(ctx: &EvalCtx) -> String {
    let Some(rid) = ctx.self_room else {
        return String::new();
    };
    let Ok(Some(self_mob)) = ctx.db.get_mobile_data(&ctx.self_id) else {
        return String::new();
    };
    let self_name = self_mob.name.to_ascii_lowercase();
    let mut names: Vec<String> = Vec::new();
    if let Ok(mobs) = ctx.db.get_mobiles_in_room(&rid) {
        for m in mobs {
            if m.id == ctx.self_id {
                continue;
            }
            if let Some(master) = m.charm_master() {
                if master.to_ascii_lowercase() == self_name {
                    names.push(m.name);
                }
            }
        }
    }
    names.join(", ")
}

/// `%self.carried_by%` / `%self.worn_by%` for SelfKind::Obj. Returns the
/// owner's name (player or mob), or empty when the item isn't carried/worn.
fn self_item_owner(ctx: &EvalCtx, equipped: bool) -> String {
    let Ok(Some(item)) = ctx.db.get_item_data(&ctx.self_id) else {
        return String::new();
    };
    let owner_id = match (&item.location, equipped) {
        (crate::types::ItemLocation::Inventory(o), false) => o.clone(),
        (crate::types::ItemLocation::Equipped(o), true) => o.clone(),
        _ => return String::new(),
    };
    // Owner id is either a player name (lowercase) or a mobile UUID
    // string. Try mobile first, fall back to character name lookup.
    if let Ok(uid) = uuid::Uuid::parse_str(&owner_id) {
        if let Ok(Some(mob)) = ctx.db.get_mobile_data(&uid) {
            return mob.name;
        }
    }
    if let Ok(Some(ch)) = ctx.db.get_character_data(&owner_id) {
        return ch.name;
    }
    owner_id
}

/// `%self.contents%` for a container item: comma-joined list of item names.
fn self_item_contents(ctx: &EvalCtx) -> String {
    ctx.db
        .get_items_in_container(&ctx.self_id)
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Shared accessor for the `arg` and `speech` heads — both are bare strings
/// with the same field surface:
/// - bare → the whole string
/// - `.car` / `.cdr` — Lisp-style head/tail (first word / rest)
/// - `.contains(needle)` — substring match → "1"/"0"
/// - `.strlen` — string length
fn resolve_text_field(s: &str, field: Option<&str>) -> String {
    match field {
        None => s.to_string(),
        Some("car") => s.split_whitespace().next().unwrap_or("").to_string(),
        Some("cdr") => s
            .split_once(char::is_whitespace)
            .map(|(_, r)| r.trim_start().to_string())
            .unwrap_or_default(),
        Some("strlen") => s.chars().count().to_string(),
        Some(f) => {
            // Call form: contains(needle).
            if let Some((fn_name, args)) = parse_field_call(f) {
                if fn_name == "contains" {
                    let needle = args.trim();
                    return if needle.is_empty() {
                        "1".to_string()
                    } else if s.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()) {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    };
                }
            }
            String::new()
        }
    }
}

fn resolve_random(field: Option<&str>, ctx: &EvalCtx) -> String {
    use rand::seq::SliceRandom;
    use rand::Rng;
    let mut rng = rand::thread_rng();
    match field {
        Some("char") => {
            // Random PC in self_room. Empty string when no eligible target.
            let Some(room_id) = ctx.self_room else {
                return String::new();
            };
            let Ok(chars) = ctx.db.list_all_characters() else {
                return String::new();
            };
            let candidates: Vec<&str> = chars
                .iter()
                .filter(|c| c.current_room_id == room_id)
                .map(|c| c.name.as_str())
                .collect();
            candidates
                .choose(&mut rng)
                .map(|s| s.to_string())
                .unwrap_or_default()
        }
        Some("dir") => {
            // Random valid (non-None) exit direction from self_room.
            let Some(room_id) = ctx.self_room else {
                return String::new();
            };
            let Ok(Some(room)) = ctx.db.get_room_data(&room_id) else {
                return String::new();
            };
            let mut dirs: Vec<&'static str> = Vec::new();
            if room.exits.north.is_some() { dirs.push("north"); }
            if room.exits.east.is_some()  { dirs.push("east"); }
            if room.exits.south.is_some() { dirs.push("south"); }
            if room.exits.west.is_some()  { dirs.push("west"); }
            if room.exits.up.is_some()    { dirs.push("up"); }
            if room.exits.down.is_some()  { dirs.push("down"); }
            dirs.choose(&mut rng).map(|d| d.to_string()).unwrap_or_default()
        }
        Some(n) => {
            // `%random.N%` returns 1..=N.
            if let Ok(n) = n.parse::<u32>() {
                if n == 0 {
                    "0".to_string()
                } else {
                    rng.gen_range(1..=n).to_string()
                }
            } else {
                String::new()
            }
        }
        None => rng.gen_range(0..1_000_000).to_string(),
    }
}

/// `%time%` / `%time.<field>%` — game-clock accessor. Reads the singleton
/// `GameTime` from the db (cheap; it's a setting blob). Bare `%time%`
/// returns hour as a stock-tbamud convenience.
///
/// Fields:
/// - `hour` (0-23), `day` (1-30), `month` (1-12), `year`
/// - `season` ("spring" / "summer" / "autumn" / "winter")
/// - `period` (dawn/morning/noon/afternoon/dusk/evening/night)
/// - `epoch` — real-world Unix seconds (NOT game time). Intended for
///   short-cooldown bookkeeping where in-game `hour` granularity is too
///   coarse: `set cooldown_until %time.epoch% + 300` / `if %actor.foo% >
///   %time.epoch%`. Monotonic across server restarts.
fn resolve_time(field: Option<&str>, ctx: &EvalCtx) -> String {
    if matches!(field, Some("epoch")) {
        return std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
    }
    let Ok(gt) = ctx.db.get_game_time() else {
        return String::new();
    };
    match field {
        None | Some("hour") => gt.hour.to_string(),
        Some("day") => gt.day.to_string(),
        Some("month") => gt.month.to_string(),
        Some("year") => gt.year.to_string(),
        Some("season") => format!("{:?}", gt.get_season()).to_ascii_lowercase(),
        Some("period") => format!("{:?}", gt.get_time_of_day()).to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// `%weather%` / `%weather.<field>%` — area-aware weather accessor.
/// Resolves the script's source room, projects the global rolled weather
/// through the room's area `ClimateProfile`, and returns the result.
///
/// Source room is `ctx.self_room` (set on mob/room/in-room item triggers).
/// When the script has no anchored room, falls back to global weather.
///
/// Fields:
/// - bare / `sky` — short slug (`clear`/`rain`/`snow`/`fog`/...)
/// - `desc` — human-readable phrase ("lightly raining")
/// - `temp` — effective temperature in Celsius (climate offset applied)
/// - `tempcat` — category slug (`freezing`/`cold`/`mild`/`hot`/...)
fn resolve_weather(field: Option<&str>, ctx: &EvalCtx) -> String {
    let Ok(gt) = ctx.db.get_game_time() else {
        return String::new();
    };
    let climate = room_climate(ctx, ctx.self_room);
    let projected = climate.project(gt.weather);
    match field {
        None | Some("sky") => weather_slug(projected).to_string(),
        Some("desc") => projected.to_string(),
        Some("temp") => {
            let base = gt.calculate_effective_temperature();
            (base + climate.temperature_offset()).to_string()
        }
        Some("tempcat") => {
            let base = gt.calculate_effective_temperature();
            let effective = base + climate.temperature_offset();
            tempcat_slug(effective).to_string()
        }
        _ => String::new(),
    }
}

fn room_climate(ctx: &EvalCtx, room_id: Option<uuid::Uuid>) -> crate::types::ClimateProfile {
    let Some(rid) = room_id else {
        return crate::types::ClimateProfile::default();
    };
    let area_id = ctx
        .db
        .get_room_data(&rid)
        .ok()
        .flatten()
        .and_then(|r| r.area_id);
    let Some(aid) = area_id else {
        return crate::types::ClimateProfile::default();
    };
    ctx.db
        .get_area_data(&aid)
        .ok()
        .flatten()
        .map(|a| a.climate)
        .unwrap_or_default()
}

fn weather_slug(w: crate::types::WeatherCondition) -> &'static str {
    use crate::types::WeatherCondition::*;
    match w {
        Clear => "clear",
        PartlyCloudy => "partly_cloudy",
        Cloudy => "cloudy",
        Overcast => "overcast",
        LightRain => "light_rain",
        Rain => "rain",
        HeavyRain => "heavy_rain",
        Thunderstorm => "thunderstorm",
        LightSnow => "light_snow",
        Snow => "snow",
        Blizzard => "blizzard",
        Fog => "fog",
    }
}

/// `%season%` — current season slug (`spring`/`summer`/`autumn`/`winter`).
fn resolve_season(ctx: &EvalCtx) -> String {
    let Ok(gt) = ctx.db.get_game_time() else {
        return String::new();
    };
    format!("{:?}", gt.get_season()).to_ascii_lowercase()
}

/// `%sunlight%` — `1` during daylight hours (dawn through dusk), `0` after
/// dark. Stock pattern: `if %sunlight%` gates undead/vampire behavior.
fn resolve_sunlight(ctx: &EvalCtx) -> String {
    let Ok(gt) = ctx.db.get_game_time() else {
        return "0".to_string();
    };
    use crate::types::TimeOfDay::*;
    let lit = matches!(
        gt.get_time_of_day(),
        Dawn | Morning | Noon | Afternoon | Dusk
    );
    if lit { "1".to_string() } else { "0".to_string() }
}

fn tempcat_slug(c: i32) -> &'static str {
    if c < 0 { "freezing" }
    else if c < 10 { "cold" }
    else if c < 15 { "cool" }
    else if c < 20 { "mild" }
    else if c < 25 { "warm" }
    else if c < 35 { "hot" }
    else { "sweltering" }
}

/// Returns `Some(value)` for known character fields (value may be the
/// empty string for legitimately-empty cases like `fighting` when not in
/// combat); returns `None` for unknown fields so callers can fall through
/// to dg_vars lookup (Phase 8a).
fn try_character_field(ch: &crate::types::CharacterData, field: &str) -> Option<String> {
    Some(match field {
        "name" => ch.name.clone(),
        "level" => ch.level.to_string(),
        "hitp" | "hp" => ch.hp.to_string(),
        "maxhp" => ch.max_hp.to_string(),
        "mana" => ch.mana.to_string(),
        "maxmana" => ch.max_mana.to_string(),
        "move" => ch.stamina.to_string(),
        "maxmove" => ch.max_stamina.to_string(),
        "gold" => ch.gold.to_string(),
        "vnum" => "-1".to_string(), // PCs have no vnum (matches tbamud).
        "is_pc" => "1".to_string(),
        // Morality slider, -200..=+200; tier thresholds at +/-100. `align`/`alignment`
        // returns the same value for Circle-script compatibility — note the scale is
        // narrower than tbamud's -1000..1000, so imported thresholds need rescaling.
        // Prefer `%actor.morality%` / `%actor.morality_tier%` in new scripts.
        "align" | "alignment" | "morality" => ch.morality.to_string(),
        "morality_tier" => crate::morality::MoralityTier::from_value(ch.morality)
            .key()
            .to_string(),
        "maxhitp" => ch.max_hp.to_string(),
        "str" | "strength" => ch.stat_str.to_string(),
        "dex" | "dexterity" => ch.stat_dex.to_string(),
        "con" | "constitution" => ch.stat_con.to_string(),
        "int" | "intelligence" => ch.stat_int.to_string(),
        "wis" | "wisdom" => ch.stat_wis.to_string(),
        "cha" | "charisma" => ch.stat_cha.to_string(),
        "class" => ch.class_name.clone(),
        "race" => ch.race.clone(),
        "sex" | "gender" => gender_or_default(&ch.gender).to_string(),
        "heshe" => subjective_pronoun(&ch.gender).to_string(),
        "himher" => objective_pronoun(&ch.gender).to_string(),
        "hisher" | "hers" => possessive_pronoun(&ch.gender).to_string(),
        "room" => ch.current_room_id.to_string(),
        "fighting" => ch
            .combat
            .targets
            .first()
            .map(|t| t.target_id.to_string())
            .unwrap_or_default(),
        "pos" | "position" => ch.position.to_string(),
        // No quest-point system — return 0.
        "questpoints" | "qp" => "0".to_string(),
        // No magical-affect listing surface yet — return empty so
        // boolean checks of `%actor.affect%` evaluate as false.
        "affect" | "affects" => String::new(),
        // No PK alignment system; killer/thief flags always 0.
        "is_killer" | "is_thief" => "0".to_string(),
        "drunk" => ch.drunk_level.to_string(),
        // PCs don't follow other entities through the charm system; this
        // field exists for symmetry with mobiles. Empty when unfollowed.
        "master" => ch.following.clone().unwrap_or_default(),
        // tbamud's `%actor.alias%` returns the player's alias string —
        // IronMUD has no alias surface, so silently return empty.
        "alias" => String::new(),
        "hunger" => ch.hunger.to_string(),
        "thirst" => ch.thirst.to_string(),
        "canbeseen" => "1".to_string(),
        _ => return None,
    })
}

fn try_mobile_field(mob: &crate::types::MobileData, field: &str) -> Option<String> {
    Some(match field {
        "name" => mob.name.clone(),
        "level" => mob.level.to_string(),
        "hitp" | "hp" => mob.current_hp.to_string(),
        "maxhp" => mob.max_hp.to_string(),
        "move" => mob.current_stamina.to_string(),
        "maxmove" => mob.max_stamina.to_string(),
        "gold" => mob.gold.to_string(),
        "vnum" => mob.vnum.clone(),
        "is_pc" => "0".to_string(),
        "align" | "alignment" => "0".to_string(),
        "maxhitp" => mob.max_hp.to_string(),
        "pos" | "position" => "standing".to_string(),
        "questpoints" | "qp" => "0".to_string(),
        "affect" | "affects" => String::new(),
        "is_killer" | "is_thief" => "0".to_string(),
        "str" | "strength" => mob.stat_str.to_string(),
        "dex" | "dexterity" => mob.stat_dex.to_string(),
        "con" | "constitution" => mob.stat_con.to_string(),
        "int" | "intelligence" => mob.stat_int.to_string(),
        "wis" | "wisdom" => mob.stat_wis.to_string(),
        "cha" | "charisma" => mob.stat_cha.to_string(),
        "sex" | "gender" => gender_or_default(mob.resolved_gender()).to_string(),
        "heshe" => subjective_pronoun(mob.resolved_gender()).to_string(),
        "himher" => objective_pronoun(mob.resolved_gender()).to_string(),
        "hisher" | "hers" => possessive_pronoun(mob.resolved_gender()).to_string(),
        "shortdesc" => mob.short_desc.clone(),
        "longdesc" => mob.long_desc.clone(),
        "room" => mob.current_room_id.map(|r| r.to_string()).unwrap_or_default(),
        "fighting" => mob
            .combat
            .targets
            .first()
            .map(|t| t.target_id.to_string())
            .unwrap_or_default(),
        "drunk" => "0".to_string(),
        // For mobs, master is the player name they're charmed by (if any).
        "master" => mob.charm_master().unwrap_or("").to_string(),
        "alias" => String::new(),
        // Sim mobs have a NeedsState (hunger/energy/comfort, 0-100). Non-sim
        // mobs return 0. NeedsState has no thirst field — always 0 for now.
        "hunger" => mob
            .needs
            .as_ref()
            .map(|n| n.hunger.to_string())
            .unwrap_or_else(|| "0".to_string()),
        "thirst" => "0".to_string(),
        "canbeseen" => "1".to_string(),
        _ => return None,
    })
}

/// Recognised gender categories. We support the four DG/tbamud pronoun
/// sets: he/him/his, she/her/her, they/them/their, it/it/its.
///
/// - `Male` / `Female` — binary gendered pronouns.
/// - `Nonbinary` — modern singular they; aliases include `nb`, `enby`,
///   `they`, `nonbinary`.
/// - `Neuter` — it/its, used for objects, undead, golems, dragons, and
///   any robot/automaton/animate-thing class. Empty/unrecognised input
///   resolves here for DG compatibility (tbamud's third sex is neuter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenderKind {
    Male,
    Female,
    Nonbinary,
    Neuter,
}

fn parse_gender(g: &str) -> GenderKind {
    match g.trim().to_ascii_lowercase().as_str() {
        "male" | "m" | "man" => GenderKind::Male,
        "female" | "f" | "woman" => GenderKind::Female,
        "nonbinary" | "non-binary" | "nb" | "enby" | "they" | "them" => GenderKind::Nonbinary,
        // "neuter" is the DG-canonical label; aliases cover obvious robot/
        // object framings and the bare pronoun forms.
        "neuter" | "it" | "object" | "thing" | "robot" | "automaton" | "construct" => {
            GenderKind::Neuter
        }
        _ => GenderKind::Neuter,
    }
}

/// Return the gender string normalized to one of `male`, `female`,
/// `nonbinary`, `neuter`. Empty / unrecognised resolves as `neuter` to
/// match tbamud DG semantics.
fn gender_or_default(g: &str) -> &'static str {
    match parse_gender(g) {
        GenderKind::Male => "male",
        GenderKind::Female => "female",
        GenderKind::Nonbinary => "nonbinary",
        GenderKind::Neuter => "neuter",
    }
}

/// `he/she/they/it` for `%actor.heshe%`.
fn subjective_pronoun(g: &str) -> &'static str {
    match parse_gender(g) {
        GenderKind::Male => "he",
        GenderKind::Female => "she",
        GenderKind::Nonbinary => "they",
        GenderKind::Neuter => "it",
    }
}

/// `him/her/them/it` for `%actor.himher%`.
fn objective_pronoun(g: &str) -> &'static str {
    match parse_gender(g) {
        GenderKind::Male => "him",
        GenderKind::Female => "her",
        GenderKind::Nonbinary => "them",
        GenderKind::Neuter => "it",
    }
}

/// `his/her/their/its` for `%actor.hisher%`.
fn possessive_pronoun(g: &str) -> &'static str {
    match parse_gender(g) {
        GenderKind::Male => "his",
        GenderKind::Female => "her",
        GenderKind::Nonbinary => "their",
        GenderKind::Neuter => "its",
    }
}

/// Resolve `%object%` / `%object.field%` against the fire-site item bound
/// in `ctx.context_vars["item_id"]`. Returns empty when the binding is
/// missing, fails to parse, or the item no longer exists.
fn resolve_object_field(ctx: &EvalCtx, field: Option<&str>) -> String {
    let Some(id_str) = ctx.context_vars.get("item_id") else {
        return String::new();
    };
    let Ok(uid) = uuid::Uuid::parse_str(id_str) else {
        return String::new();
    };
    let Ok(Some(item)) = ctx.db.get_item_data(&uid) else {
        return String::new();
    };
    let Some(f) = field else {
        // Bare `%object%` → item name (stock tbamud convention).
        return item.name.clone();
    };
    if f == "id" {
        return item.id.to_string();
    }
    if let Some(v) = try_item_field(&item, f) {
        return v;
    }
    item.dg_vars.get(f).cloned().unwrap_or_default()
}

fn try_item_field(item: &crate::types::ItemData, field: &str) -> Option<String> {
    Some(match field {
        "name" => item.name.clone(),
        "vnum" => item.vnum.clone().unwrap_or_default(),
        "weight" => item.weight.to_string(),
        "type" => format!("{:?}", item.item_type),
        "shortdesc" => item.short_desc.clone(),
        "longdesc" => item.long_desc.clone(),
        // Item decay timer (Phase 8e). Lives on dg_vars so we don't need
        // a struct field; otimer writes it, %self.timer% reads it. 0 when
        // unset.
        "timer" => item
            .dg_vars
            .get("timer")
            .cloned()
            .unwrap_or_else(|| "0".to_string()),
        // tbamud value slots — not modeled per slot in IronMUD. Return 0.
        "val0" | "val1" | "val2" | "val3" => "0".to_string(),
        "cost" => item.value.to_string(),
        _ => return None,
    })
}

fn room_field(room: &crate::types::RoomData, field: &str) -> String {
    try_room_field(room, field).unwrap_or_default()
}

fn try_room_field(room: &crate::types::RoomData, field: &str) -> Option<String> {
    Some(match field {
        "name" | "title" => room.title.clone(),
        "vnum" => room.vnum.clone().unwrap_or_default(),
        "id" => room.id.to_string(),
        "description" | "desc" => room.description.clone(),
        // Direction-as-field: returns the destination room id for that
        // exit, or empty when no exit. Stock pattern: `if %self.north%`
        // is "is there a north exit?".
        "north" => room.exits.north.map(|r| r.to_string()).unwrap_or_default(),
        "south" => room.exits.south.map(|r| r.to_string()).unwrap_or_default(),
        "east" => room.exits.east.map(|r| r.to_string()).unwrap_or_default(),
        "west" => room.exits.west.map(|r| r.to_string()).unwrap_or_default(),
        "up" => room.exits.up.map(|r| r.to_string()).unwrap_or_default(),
        "down" => room.exits.down.map(|r| r.to_string()).unwrap_or_default(),
        _ => return None,
    })
}

/// Parse `field(args)` into `(fn_name, args)`. Returns None when no parens.
fn parse_field_call(field: &str) -> Option<(&str, &str)> {
    let open = field.find('(')?;
    let close = field.rfind(')')?;
    if close <= open {
        return None;
    }
    Some((&field[..open], &field[open + 1..close]))
}

/// Read-only call accessors that don't fit the integer-mutator pattern.
/// Used by both `apply_actor_call` and `apply_self_call`.
fn is_reader_call(fn_name: &str) -> bool {
    matches!(
        fn_name,
        "varexists"
            | "has_item"
            | "eq"
            | "inventory"
            | "equipped"
            | "affect"
            | "affects"
            | "door"
            | "skill"
    )
}

/// Normalize a DG direction token. Accepts the canonical full names plus the
/// one-letter shortcuts builders commonly type. Returns `""` for anything
/// else so callers can short-circuit.
fn normalize_dg_direction(dir: &str) -> &'static str {
    match dir.trim().to_ascii_lowercase().as_str() {
        "n" | "north" => "north",
        "s" | "south" => "south",
        "e" | "east" => "east",
        "w" | "west" => "west",
        "u" | "up" => "up",
        "d" | "down" => "down",
        _ => "",
    }
}

/// Read a single field off the door on `room`'s `<dir>` exit. The arg
/// is `<dir>,<field>` (whitespace tolerated). Fields:
///
/// - `exists` → "1"/"0" — is there a door at all in that direction?
/// - `locked` / `unlocked` → "1"/"0" — current lock state.
/// - `closed` / `open` → "1"/"0" — current closure state.
/// - `pickproof` → "1"/"0".
/// - `name` → door name string (e.g. "gate").
/// - `key` / `key_vnum` → key vnum string, empty when no key set.
///
/// Missing-door cases return "0" for the boolean-shaped fields and the
/// empty string for the string-shaped ones, so `if` checks compose
/// naturally without needing a separate `exists` guard.
fn read_door_field(
    doors: &std::collections::HashMap<String, crate::types::DoorState>,
    args: &str,
) -> String {
    let mut it = args.splitn(2, ',');
    let dir_tok = it.next().unwrap_or("").trim();
    let field = it.next().unwrap_or("").trim().to_ascii_lowercase();
    let dir = normalize_dg_direction(dir_tok);
    if dir.is_empty() || field.is_empty() {
        return String::new();
    }
    let Some(door) = doors.get(dir) else {
        return match field.as_str() {
            "exists" | "locked" | "unlocked" | "closed" | "open" | "pickproof" => {
                "0".to_string()
            }
            _ => String::new(),
        };
    };
    let bool01 = |b: bool| if b { "1" } else { "0" }.to_string();
    match field.as_str() {
        "exists" => "1".to_string(),
        "locked" => bool01(door.is_locked),
        "unlocked" => bool01(!door.is_locked),
        "closed" => bool01(door.is_closed),
        "open" => bool01(!door.is_closed),
        "pickproof" => bool01(door.pickproof),
        "name" => door.name.clone(),
        "key" | "key_vnum" => door.key_vnum.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

/// Mutating accessors that IronMUD doesn't model (positions, wait state,
/// quest points). Recognised so the analyzer doesn't flag them and the
/// runtime silently no-ops with empty result.
fn is_unmodeled_mutator(fn_name: &str) -> bool {
    matches!(fn_name, "pos" | "position" | "wait" | "questpoints" | "qp")
}

/// Call-form dispatch on `%actor.<fn>(<args>)%`. Routes to a reader
/// (varexists/has_item/eq) or a mutator (gold/hitp/move/exp).
fn apply_actor_call(
    actor: Option<&ActorRef>,
    fn_name: &str,
    args: &str,
    ctx: &EvalCtx,
    state: &State,
) -> String {
    let Some(actor) = actor else {
        return String::new();
    };
    if is_reader_call(fn_name) {
        return read_actor_call(actor, fn_name, args, ctx, state);
    }
    if is_unmodeled_mutator(fn_name) {
        return String::new();
    }
    let n: i32 = args.trim().parse().unwrap_or(0);
    match actor {
        ActorRef::Player { name, .. } => mutate_character(ctx, name, fn_name, n),
        ActorRef::Mob { mobile_id, .. } => mutate_mobile(ctx, mobile_id, fn_name, n),
    }
}

/// `%self.<fn>(<args>)%` — readers + mutators on the bound self entity.
fn apply_self_call(fn_name: &str, args: &str, ctx: &EvalCtx, state: &State) -> String {
    if is_reader_call(fn_name) {
        return read_self_call(fn_name, args, ctx, state);
    }
    if is_unmodeled_mutator(fn_name) {
        return String::new();
    }
    let n: i32 = args.trim().parse().unwrap_or(0);
    match ctx.self_kind {
        SelfKind::Mob => mutate_mobile(ctx, &ctx.self_id, fn_name, n),
        // Items / rooms have no gold/hp/move semantics; silently no-op.
        _ => String::new(),
    }
}

/// Read-only call dispatch for actors. Returns:
/// - `varexists(name)` → "1"/"0" if the actor has that var on its dg_vars.
/// - `has_item(vnum)`  → "1"/"0" if the actor's inventory or equipment
///   contains an item with the given vnum (string match, ignoring case).
/// - `eq(slot)`        → name of equipped item in `slot` (Phase 7c). Slots
///   aren't modeled on PCs/mobs in IronMUD beyond Equipped(owner), so
///   `eq(*)` returns the first equipped item's name. `eq(wield)` and
///   `eq(hold)` are common — both fall through here.
fn read_actor_call(
    actor: &ActorRef,
    fn_name: &str,
    args: &str,
    ctx: &EvalCtx,
    state: &State,
) -> String {
    let arg = args.trim();
    match fn_name {
        "varexists" => {
            let exists = match actor {
                ActorRef::Player { name, .. } => ctx
                    .db
                    .get_character_data(name)
                    .ok()
                    .flatten()
                    .map(|c| c.dg_vars.contains_key(arg))
                    .unwrap_or(false),
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_mobile_data(mobile_id)
                    .ok()
                    .flatten()
                    .map(|m| m.dg_vars.contains_key(arg))
                    .unwrap_or(false),
            };
            // tbamud's varexists also returns true if the var is set as a
            // local in the running script — common idiom is to set+check.
            if exists || state.locals.contains_key(arg) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        "has_item" => {
            let owner_id = match actor {
                ActorRef::Player { name, .. } => name.clone(),
                ActorRef::Mob { mobile_id, .. } => mobile_id.to_string(),
            };
            if actor_has_item_with_vnum(ctx, actor, &owner_id, arg) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        "eq" => {
            let equipped = match actor {
                ActorRef::Player { name, .. } => {
                    ctx.db.get_equipped_items(name).unwrap_or_default()
                }
                ActorRef::Mob { mobile_id, .. } => {
                    ctx.db.get_items_equipped_on_mobile(mobile_id).unwrap_or_default()
                }
            };
            // Slot semantics: parse arg via WearLocation::from_str. If the
            // slot is recognised, return the equipped item whose
            // `currently_worn_at` matches. Empty / unrecognised arg falls
            // back to first-equipped (preserves Phase 7c `if %actor.eq(*)%`
            // boolean-shape callers). Old saves with no currently_worn_at
            // set return empty for slot queries until re-equipped.
            let slot = crate::types::WearLocation::from_str(arg);
            match slot {
                Some(s) => equipped
                    .iter()
                    .find(|i| i.currently_worn_at == Some(s))
                    .map(|i| i.name.clone())
                    .unwrap_or_default(),
                None => equipped.first().map(|i| i.name.clone()).unwrap_or_default(),
            }
        }
        // `%actor.inventory(vnum)%` — count items in actor's inventory
        // with the given vnum. Stock pattern: `if %actor.inventory(82)%`
        // checks "has at least one of item 82". Returns the count as a
        // string so > 0 is truthy.
        "inventory" => {
            let inv = match actor {
                ActorRef::Player { name, .. } => {
                    ctx.db.get_items_in_inventory(name).unwrap_or_default()
                }
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_items_in_mobile_inventory(mobile_id)
                    .unwrap_or_default(),
            };
            inv.iter()
                .filter(|i| {
                    i.vnum
                        .as_deref()
                        .map(|v| v.eq_ignore_ascii_case(arg))
                        .unwrap_or(false)
                })
                .count()
                .to_string()
        }
        // `%actor.equipped(vnum)%` — count items currently equipped on the
        // actor with the given vnum. Mirror of `inventory(vnum)` but for
        // worn/wielded items. Used for armor-set detection patterns like
        // `if %actor.equipped(3010)% >= 2` (matching gloves bonus).
        "equipped" => {
            let eq = match actor {
                ActorRef::Player { name, .. } => {
                    ctx.db.get_equipped_items(name).unwrap_or_default()
                }
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_items_equipped_on_mobile(mobile_id)
                    .unwrap_or_default(),
            };
            eq.iter()
                .filter(|i| {
                    i.vnum
                        .as_deref()
                        .map(|v| v.eq_ignore_ascii_case(arg))
                        .unwrap_or(false)
                })
                .count()
                .to_string()
        }
        // `%actor.skill(<key>)%` — effective value of a builder-published
        // custom skill: actor's base `custom_skills[key]` plus the sum of
        // matching `EffectType::CustomSkillBoost` buffs. Returns "0" for
        // unknown actors or absent keys — graceful, mirrors other readers.
        "skill" => {
            let key = arg.to_ascii_lowercase();
            let (base, buffs) = match actor {
                ActorRef::Player { name, .. } => ctx
                    .db
                    .get_character_data(name)
                    .ok()
                    .flatten()
                    .map(|c| {
                        (
                            c.custom_skills.get(&key).copied().unwrap_or(0),
                            c.active_buffs.clone(),
                        )
                    })
                    .unwrap_or((0, Vec::new())),
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_mobile_data(mobile_id)
                    .ok()
                    .flatten()
                    .map(|m| {
                        (
                            m.custom_skills.get(&key).copied().unwrap_or(0),
                            m.active_buffs.clone(),
                        )
                    })
                    .unwrap_or((0, Vec::new())),
            };
            let bonus: i32 = buffs
                .iter()
                .filter(|b| {
                    b.effect_type == crate::types::EffectType::CustomSkillBoost
                        && b.skill_key.as_deref().map(|s| s.eq_ignore_ascii_case(&key))
                            == Some(true)
                })
                .map(|b| b.magnitude)
                .sum();
            (base + bonus).to_string()
        }
        // `%actor.affect(spell)%` — predicate for "is the actor affected
        // by an effect named `spell`?". Checks active_buffs for a matching
        // EffectType. Returns "1"/"0".
        "affect" | "affects" => {
            let buffs = match actor {
                ActorRef::Player { name, .. } => ctx
                    .db
                    .get_character_data(name)
                    .ok()
                    .flatten()
                    .map(|c| c.active_buffs.clone())
                    .unwrap_or_default(),
                ActorRef::Mob { mobile_id, .. } => ctx
                    .db
                    .get_mobile_data(mobile_id)
                    .ok()
                    .flatten()
                    .map(|m| m.active_buffs.clone())
                    .unwrap_or_default(),
            };
            let needle = arg.to_ascii_lowercase();
            if buffs
                .iter()
                .any(|b| format!("{:?}", b.effect_type).to_ascii_lowercase() == needle)
            {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        _ => String::new(),
    }
}

fn read_self_call(fn_name: &str, args: &str, ctx: &EvalCtx, state: &State) -> String {
    let arg = args.trim();
    match (ctx.self_kind, fn_name) {
        (SelfKind::Mob, "varexists") => {
            let exists = ctx
                .db
                .get_mobile_data(&ctx.self_id)
                .ok()
                .flatten()
                .map(|m| m.dg_vars.contains_key(arg))
                .unwrap_or(false);
            if exists || state.locals.contains_key(arg) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        (SelfKind::Obj, "varexists") => {
            let exists = ctx
                .db
                .get_item_data(&ctx.self_id)
                .ok()
                .flatten()
                .map(|i| i.dg_vars.contains_key(arg))
                .unwrap_or(false);
            if exists || state.locals.contains_key(arg) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        (SelfKind::Room, "varexists") => {
            let exists = ctx
                .db
                .get_room_data(&ctx.self_id)
                .ok()
                .flatten()
                .map(|r| r.dg_vars.contains_key(arg))
                .unwrap_or(false);
            if exists || state.locals.contains_key(arg) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        (SelfKind::Mob, "has_item") => {
            let owner_id = ctx.self_id.to_string();
            if mobile_has_item_with_vnum(ctx, &ctx.self_id, &owner_id, arg) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        (SelfKind::Mob, "eq") => {
            let equipped = ctx
                .db
                .get_items_equipped_on_mobile(&ctx.self_id)
                .unwrap_or_default();
            // Mirror of actor-side eq(slot): slot-aware when arg parses
            // as a WearLocation; otherwise first-equipped fallback.
            match crate::types::WearLocation::from_str(arg) {
                Some(s) => equipped
                    .iter()
                    .find(|i| i.currently_worn_at == Some(s))
                    .map(|i| i.name.clone())
                    .unwrap_or_default(),
                None => equipped.first().map(|i| i.name.clone()).unwrap_or_default(),
            }
        }
        // `%self.equipped(vnum)%` on a mob — count of equipped items
        // matching that vnum. Mirror of the actor-side accessor in
        // `read_actor_call`. Used by armor-set-style triggers attached
        // to mobs.
        (SelfKind::Mob, "equipped") => ctx
            .db
            .get_items_equipped_on_mobile(&ctx.self_id)
            .unwrap_or_default()
            .iter()
            .filter(|i| {
                i.vnum
                    .as_deref()
                    .map(|v| v.eq_ignore_ascii_case(arg))
                    .unwrap_or(false)
            })
            .count()
            .to_string(),
        // `%self.door(<dir>, <field>)%` — inspect the door on self's room
        // exit. For a mob, "self's room" is the mob's current room; for a
        // room-self, it's the room itself. See `read_door_field` for the
        // field vocabulary.
        (SelfKind::Mob, "door") => ctx
            .db
            .get_mobile_data(&ctx.self_id)
            .ok()
            .flatten()
            .and_then(|m| m.current_room_id)
            .and_then(|rid| ctx.db.get_room_data(&rid).ok().flatten())
            .map(|room| read_door_field(&room.doors, arg))
            .unwrap_or_default(),
        (SelfKind::Room, "door") => ctx
            .db
            .get_room_data(&ctx.self_id)
            .ok()
            .flatten()
            .map(|room| read_door_field(&room.doors, arg))
            .unwrap_or_default(),
        // `%self.skill(<key>)%` on a mob — effective custom skill value.
        // Mirror of actor-side `skill(key)`.
        (SelfKind::Mob, "skill") => {
            let key = arg.to_ascii_lowercase();
            let (base, buffs) = ctx
                .db
                .get_mobile_data(&ctx.self_id)
                .ok()
                .flatten()
                .map(|m| {
                    (
                        m.custom_skills.get(&key).copied().unwrap_or(0),
                        m.active_buffs.clone(),
                    )
                })
                .unwrap_or((0, Vec::new()));
            let bonus: i32 = buffs
                .iter()
                .filter(|b| {
                    b.effect_type == crate::types::EffectType::CustomSkillBoost
                        && b.skill_key.as_deref().map(|s| s.eq_ignore_ascii_case(&key))
                            == Some(true)
                })
                .map(|b| b.magnitude)
                .sum();
            (base + bonus).to_string()
        }
        _ => String::new(),
    }
}

/// Searches actor's inventory + equipped items for one whose vnum equals
/// `vnum_or_keyword` (string equality, case-insensitive). Stock tbamud
/// passes integer vnums; callers can also pass keywords if they want a
/// looser match — we don't currently do keyword-fallback.
fn actor_has_item_with_vnum(
    ctx: &EvalCtx,
    actor: &ActorRef,
    owner_id: &str,
    vnum_or_keyword: &str,
) -> bool {
    let needle = vnum_or_keyword.trim();
    if needle.is_empty() {
        return false;
    }
    match actor {
        ActorRef::Player { name, .. } => {
            let inv = ctx.db.get_items_in_inventory(name).unwrap_or_default();
            let eq = ctx.db.get_equipped_items(name).unwrap_or_default();
            inv.iter().chain(eq.iter()).any(|i| {
                i.vnum
                    .as_deref()
                    .map(|v| v.eq_ignore_ascii_case(needle))
                    .unwrap_or(false)
            })
        }
        ActorRef::Mob { mobile_id, .. } => {
            mobile_has_item_with_vnum(ctx, mobile_id, owner_id, needle)
        }
    }
}

fn mobile_has_item_with_vnum(
    ctx: &EvalCtx,
    mobile_id: &uuid::Uuid,
    _owner_id: &str,
    vnum: &str,
) -> bool {
    let inv = ctx.db.get_items_in_mobile_inventory(mobile_id).unwrap_or_default();
    let eq = ctx.db.get_items_equipped_on_mobile(mobile_id).unwrap_or_default();
    inv.iter().chain(eq.iter()).any(|i| {
        i.vnum
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case(vnum))
            .unwrap_or(false)
    })
}

fn mutate_character(ctx: &EvalCtx, name: &str, fn_name: &str, n: i32) -> String {
    let Ok(Some(mut ch)) = ctx.db.get_character_data(name) else {
        return String::new();
    };
    let new_val = match fn_name {
        "gold" => {
            ch.gold = (ch.gold + n).max(0);
            ch.gold
        }
        "hitp" | "hp" => {
            ch.hp = (ch.hp + n).clamp(0, ch.max_hp);
            ch.hp
        }
        "move" => {
            ch.stamina = (ch.stamina + n).clamp(0, ch.max_stamina);
            ch.stamina
        }
        "drunk" => {
            ch.drunk_level = (ch.drunk_level + n).clamp(0, 100);
            ch.drunk_level
        }
        "hunger" => {
            ch.hunger = (ch.hunger + n).clamp(0, ch.max_hunger);
            ch.hunger
        }
        "thirst" => {
            ch.thirst = (ch.thirst + n).clamp(0, ch.max_thirst);
            ch.thirst
        }
        "exp" => {
            // Players have no top-level `exp` field; treat as no-op for
            // now and return 0. (Phase 6 candidate: thread XP through
            // ProgressionState if we end up needing it.)
            0
        }
        _ => return String::new(),
    };
    let _ = ctx.db.save_character_data(ch);
    new_val.to_string()
}

fn mutate_mobile(ctx: &EvalCtx, mobile_id: &uuid::Uuid, fn_name: &str, n: i32) -> String {
    let Ok(Some(mut mob)) = ctx.db.get_mobile_data(mobile_id) else {
        return String::new();
    };
    let new_val = match fn_name {
        "gold" => {
            mob.gold = (mob.gold + n).max(0);
            mob.gold
        }
        "hitp" | "hp" => {
            mob.current_hp = (mob.current_hp + n).clamp(0, mob.max_hp);
            mob.current_hp
        }
        "move" => {
            mob.current_stamina = (mob.current_stamina + n).clamp(0, mob.max_stamina);
            mob.current_stamina
        }
        // Mobs don't carry XP — no-op.
        "exp" => 0,
        _ => return String::new(),
    };
    let _ = ctx.db.save_mobile_data(mob);
    new_val.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pronoun_helpers_resolve_table() {
        // Table-driven: gender × { subj, obj, poss }.
        for &(g, subj, obj, poss) in &[
            // Male.
            ("male", "he", "him", "his"),
            ("M", "he", "him", "his"),
            ("man", "he", "him", "his"),
            // Female.
            ("female", "she", "her", "her"),
            ("F", "she", "her", "her"),
            ("woman", "she", "her", "her"),
            // Nonbinary — modern singular they.
            ("nonbinary", "they", "them", "their"),
            ("non-binary", "they", "them", "their"),
            ("nb", "they", "them", "their"),
            ("enby", "they", "them", "their"),
            ("they", "they", "them", "their"),
            ("Them", "they", "them", "their"),
            // Neuter — robots, automatons, objects, undefined.
            ("neuter", "it", "it", "its"),
            ("it", "it", "it", "its"),
            ("robot", "it", "it", "its"),
            ("automaton", "it", "it", "its"),
            ("construct", "it", "it", "its"),
            ("object", "it", "it", "its"),
            ("", "it", "it", "its"),
            ("garbage", "it", "it", "its"),
        ] {
            assert_eq!(subjective_pronoun(g), subj, "subj({})", g);
            assert_eq!(objective_pronoun(g), obj, "obj({})", g);
            assert_eq!(possessive_pronoun(g), poss, "poss({})", g);
        }
    }

    #[test]
    fn gender_or_default_normalizes_input() {
        assert_eq!(gender_or_default("Male"), "male");
        assert_eq!(gender_or_default("F"), "female");
        assert_eq!(gender_or_default("nb"), "nonbinary");
        assert_eq!(gender_or_default("They"), "nonbinary");
        assert_eq!(gender_or_default("robot"), "neuter");
        assert_eq!(gender_or_default(""), "neuter");
        assert_eq!(gender_or_default("nonsense"), "neuter");
    }

    #[test]
    fn parse_field_call_extracts_name_and_args() {
        assert_eq!(parse_field_call("gold(-50)"), Some(("gold", "-50")));
        assert_eq!(parse_field_call("hitp(10)"), Some(("hitp", "10")));
        assert_eq!(parse_field_call("plain"), None);
        // Mismatched parens.
        assert_eq!(parse_field_call("gold("), None);
    }

    #[test]
    fn normalize_dg_direction_accepts_short_and_long() {
        for (input, want) in [
            ("n", "north"),
            ("N", "north"),
            ("north", "north"),
            (" North ", "north"),
            ("e", "east"),
            ("EAST", "east"),
            ("u", "up"),
            ("down", "down"),
            ("", ""),
            ("northwest", ""),
            ("bogus", ""),
        ] {
            assert_eq!(normalize_dg_direction(input), want, "input={input:?}");
        }
    }

    fn doors_with_east(d: crate::types::DoorState)
        -> std::collections::HashMap<String, crate::types::DoorState>
    {
        let mut m = std::collections::HashMap::new();
        m.insert("east".into(), d);
        m
    }

    #[test]
    fn read_door_field_returns_state_fields() {
        let door = crate::types::DoorState {
            name: "gate".into(),
            is_closed: false,
            is_locked: false,
            key_vnum: Some("3001".into()),
            description: None,
            keywords: vec![],
            pickproof: false,
        };
        let doors = doors_with_east(door);
        assert_eq!(read_door_field(&doors, "east, exists"), "1");
        assert_eq!(read_door_field(&doors, "east, open"), "1");
        assert_eq!(read_door_field(&doors, "east, closed"), "0");
        assert_eq!(read_door_field(&doors, "east, locked"), "0");
        assert_eq!(read_door_field(&doors, "east, unlocked"), "1");
        assert_eq!(read_door_field(&doors, "east, name"), "gate");
        assert_eq!(read_door_field(&doors, "east, key"), "3001");
        assert_eq!(read_door_field(&doors, "east, key_vnum"), "3001");
        // Short-form direction also works.
        assert_eq!(read_door_field(&doors, "e, locked"), "0");
    }

    #[test]
    fn read_door_field_locked_closed_reflect_state() {
        let door = crate::types::DoorState {
            name: "door".into(),
            is_closed: true,
            is_locked: true,
            key_vnum: None,
            description: None,
            keywords: vec![],
            pickproof: true,
        };
        let doors = doors_with_east(door);
        assert_eq!(read_door_field(&doors, "east, closed"), "1");
        assert_eq!(read_door_field(&doors, "east, open"), "0");
        assert_eq!(read_door_field(&doors, "east, locked"), "1");
        assert_eq!(read_door_field(&doors, "east, unlocked"), "0");
        assert_eq!(read_door_field(&doors, "east, pickproof"), "1");
        assert_eq!(read_door_field(&doors, "east, key"), "");
    }

    #[test]
    fn read_door_field_missing_door_returns_zero_for_bools() {
        let doors: std::collections::HashMap<String, crate::types::DoorState> =
            std::collections::HashMap::new();
        assert_eq!(read_door_field(&doors, "east, exists"), "0");
        assert_eq!(read_door_field(&doors, "east, locked"), "0");
        assert_eq!(read_door_field(&doors, "east, closed"), "0");
        // String-shaped fields return empty when no door exists.
        assert_eq!(read_door_field(&doors, "east, name"), "");
        assert_eq!(read_door_field(&doors, "east, key"), "");
    }

    #[test]
    fn read_door_field_bad_input_returns_empty() {
        let mut doors: std::collections::HashMap<String, crate::types::DoorState> =
            std::collections::HashMap::new();
        // Missing field.
        assert_eq!(read_door_field(&doors, "east"), "");
        assert_eq!(read_door_field(&doors, "east,"), "");
        // Bad direction.
        assert_eq!(read_door_field(&doors, "northwest, locked"), "");
        // Unknown field on an existing door returns empty (not "0").
        doors.insert("east".into(), crate::types::DoorState::default());
        assert_eq!(read_door_field(&doors, "east, sploded"), "");
    }
}

//! Mob world-command dispatch for DG Scripts.
//!
//! Stock tbamud assumes a mob can issue any of ~30 game verbs (`say`,
//! `emote`, `give`, `kill`, …). Triggers like:
//!
//! ```text
//! say Welcome to my shop!
//! emote bows politely.
//! give shield $n
//! ```
//!
//! work because the C-side mob_command interpreter recognises these. We
//! mirror that surface here, calling existing IronMUD primitives directly
//! rather than threading through the rhai engine. Self-bound to
//! [`SelfKind::Mob`] — obj/room triggers using these verbs are no-oped at
//! the dispatch layer.
//!
//! Verb coverage (Phase 5c initial set):
//! - communication: `say`, `tell`, `emote`, `gemote`, `pemote`, plus the
//!   stock social emotes (`smile`, `nod`, `grin`, `bow`, `cry`, `wave`,
//!   `frown`, `wink`, `shake`, `laugh`).
//! - inventory: `give <item> <player>`, `drop <item>`, `get <item>`.
//! - combat: `kill <target>`.
//! - doors: `open`, `close`, `lock`, `unlock`.
//! - movement: `goto <room>`, `flee`.
//! - info-only: `look` (no-op for mobs).

use uuid::Uuid;

use super::{ActorRef, EvalCtx, SelfKind};

/// Dispatch a mob-issued verb. Returns `true` if the verb was recognised
/// and handled (regardless of outcome — a `give` with no matching item
/// still counts as handled). Returns `false` for unknown verbs so the
/// caller can fall through to its own debug logging.
pub fn try_dispatch(verb: &str, rest: &str, ctx: &EvalCtx) -> bool {
    if ctx.self_kind != SelfKind::Mob {
        return false;
    }
    let v = verb.trim().to_ascii_lowercase();
    match v.as_str() {
        "say" => {
            do_say(rest, ctx);
            true
        }
        "tell" => {
            do_tell(rest, ctx);
            true
        }
        "emote" | "gemote" | "pemote" => {
            do_emote(rest, ctx);
            true
        }
        "give" => {
            do_give(rest, ctx);
            true
        }
        "drop" => {
            do_drop(rest, ctx);
            true
        }
        "get" => {
            do_get(rest, ctx);
            true
        }
        "kill" | "hit" | "attack" | "mkill" => {
            do_kill(rest, ctx);
            true
        }
        // Posture verbs — info-only / flavor for now. Mobs in IronMUD don't
        // track posture (they don't sleep through combat); broadcast and
        // accept the verb so the body's intent reads correctly.
        "stand" | "sit" | "rest" | "sleep" | "wake" => {
            let act = match v.as_str() {
                "stand" => "stands up",
                "sit" => "sits down",
                "rest" => "lies down to rest",
                "sleep" => "lies down and goes to sleep",
                "wake" => "wakes up",
                _ => return false,
            };
            broadcast(
                ctx,
                &format!("{} {}.", capitalize_first(&ctx.self_name), act),
            );
            true
        }
        // Extract is purge with a different name.
        "extract" => {
            let _ = ctx.db.delete_mobile(&ctx.self_id);
            true
        }
        // Follow/assist/mfollow — no-ops for now (mob-side following exists
        // through the migration system but stock tbamud uses these mostly
        // for flavor companion mobs which we don't simulate).
        "follow" | "fol" | "mfollow" | "assist" => true,
        // `take` is an alias for `get`. `junk` discards (delete) the named
        // item from the mob's inventory.
        "take" => {
            do_get(rest, ctx);
            true
        }
        "junk" => {
            do_junk(rest, ctx);
            true
        }
        // `asound` — broadcast a sound to neighbouring rooms. Implemented
        // as a flat broadcast to every room reachable via one exit step,
        // skipping self_room. Used by stock for ambient cues ("You hear
        // someone shout in the distance.").
        "asound" => {
            do_asound(rest, ctx);
            true
        }
        // `hold` is wear in Diku terms (offhand light source slot in some
        // muds). We have no slot system on mobs, so treat as wear.
        "hold" => {
            do_wear(rest, ctx, crate::types::ItemTriggerType::OnWear);
            true
        }
        // Directional movement verbs — mobs taking a step in a direction
        // is equivalent to goto-via-exit. Look up the exit on the mob's
        // current room and move into it.
        "north" | "south" | "east" | "west" | "up" | "down" => {
            do_walk(&v, ctx);
            true
        }
        "open" | "close" | "lock" | "unlock" => {
            do_door(&v, rest, ctx);
            true
        }
        "goto" => {
            do_goto(rest, ctx);
            true
        }
        "flee" => {
            do_flee(ctx);
            true
        }
        "look" => true, // info-only; mobs see via internal state already
        "consider" => true, // info-only — mob ponders silently
        "wear" => {
            do_wear(rest, ctx, crate::types::ItemTriggerType::OnWear);
            true
        }
        "wield" => {
            do_wear(rest, ctx, crate::types::ItemTriggerType::OnWield);
            true
        }
        "remove" => {
            do_remove(rest, ctx);
            true
        }
        "quaff" => {
            do_quaff(rest, ctx);
            true
        }
        // Combat-flavour verbs that don't model cleanly onto IronMUD's
        // combat — accept and silently no-op so the trigger body's intent
        // (the verb existed and ran) stays correct without spurious
        // broadcast.
        "rescue" | "disarm" | "bash" | "passdown" => true,
        // Shopkeeper flavor verbs. Mob trigger context — these advertise
        // wares/prices to the room rather than execute a real transaction
        // (which requires a player counterparty going through the player
        // shop commands).
        "list" => {
            do_shop_list(ctx);
            true
        }
        "value" => {
            do_shop_value(rest, ctx);
            true
        }
        // `sell` / `buy` from a mob trigger have no real player counterparty
        // to settle gold against — stock tbamud uses these for flavor that
        // never actually moves money. Silent no-op.
        "sell" | "buy" => true,
        // Item-interaction verbs. `light` toggles a torch/lamp the mob
        // holds; `eat` / `drink` consume a food/drink item from inventory.
        "light" => {
            do_light(rest, ctx);
            true
        }
        "eat" | "drink" => {
            do_consume(rest, ctx);
            true
        }
        // `use` activates a held wand/staff. Implemented as a flavor
        // broadcast for now; mob-side wand spell-casting needs the cast
        // engine threaded through which is out of scope here.
        "use" => {
            do_use(rest, ctx);
            true
        }
        // `order <target> <command>` — proxy a command to a charmed mob in
        // the same room, mirroring the player-side `order` skill.
        "order" => {
            do_order(rest, ctx);
            true
        }
        // Info / builder / rare custom verbs. Silent no-ops.
        "time" | "date" | "oset" | "adjust" | "pat" | "snd" => true,
        _ => {
            if let Some(text) = social_emote_text(&v, &ctx.self_name) {
                if !text.is_empty() {
                    broadcast(ctx, &text);
                }
                true
            } else {
                false
            }
        }
    }
}

/// Verbs this module recognises. Used by the static analyzer so a
/// trigger body that does `say "hi"` doesn't get flagged as unknown.
pub fn known_verbs() -> &'static [&'static str] {
    &[
        "say", "tell", "emote", "gemote", "pemote", "give", "drop", "get",
        "kill", "hit", "attack", "mkill", "open", "close", "lock", "unlock",
        "goto", "flee", "look",
        // Phase 6d: equipment + consumables
        "wear", "wield", "remove", "quaff", "consider", "hold",
        // Phase 6d follow-up: posture + lifecycle + grouping
        "stand", "sit", "rest", "sleep", "wake",
        "extract", "follow", "fol", "mfollow", "assist", "take", "junk",
        // Ambient + movement
        "asound",
        "north", "south", "east", "west", "up", "down",
        // socials
        "smile", "nod", "grin", "bow", "cry", "wave", "frown", "wink",
        "shake", "laugh", "growl", "dance", "clap", "sigh", "poke",
        "roll", "hug", "chuckle", "yawn", "whisper", "sing", "lick",
        "kiss", "cackle", "smirk", "slap", "snarl", "strut", "hum",
        "ponder", "sniff", "spit", "scream",
        // Phase 8f socials + accepted combat-flavour verbs
        "shout", "beam", "em", "flex", "giggle", "glare", "grumble",
        "mumble", "peer", "pout", "ogle", "ruffle", "drool", "shiver",
        "sneeze", "hiss", "grimace", "eyebrow", "gaze", "caress", "pet",
        "bounce", "squeeze", "think", "welcome", "great", "mua", "muah",
        "play", "sac", "sacrifice", "rem",
        "rescue", "disarm", "bash", "passdown",
        // Phase 9b: stock CircleMUD verb stubs (no-op accepted)
        "light", "drink", "eat", "sell", "value", "buy", "list",
        "time", "date", "order", "oset", "adjust", "use", "pat", "snd",
    ]
}

fn broadcast(ctx: &EvalCtx, text: &str) {
    let Some(room_id) = ctx.self_room else {
        return;
    };
    let mut t = text.to_string();
    if !t.ends_with('\n') {
        t.push('\n');
    }
    crate::broadcast_to_room(&ctx.connections, room_id, t, None);
}

fn do_say(rest: &str, ctx: &EvalCtx) {
    let msg = rest.trim();
    if msg.is_empty() {
        return;
    }
    let line = format!("{} says, \"{}\"", capitalize_first(&ctx.self_name), msg);
    broadcast(ctx, &line);
}

fn do_emote(rest: &str, ctx: &EvalCtx) {
    let msg = rest.trim();
    if msg.is_empty() {
        return;
    }
    let line = format!("{} {}", capitalize_first(&ctx.self_name), msg);
    broadcast(ctx, &line);
}

fn do_tell(rest: &str, ctx: &EvalCtx) {
    let (target_tok, msg) = split2(rest);
    if target_tok.is_empty() || msg.is_empty() {
        return;
    }
    // Resolve target by name → connection lookup.
    if let Some(cid) = crate::session::find_player_connection_by_name(&ctx.connections, &target_tok) {
        let line = format!("{} tells you, \"{}\"\n", capitalize_first(&ctx.self_name), msg);
        crate::send_client_message(&ctx.connections, cid.to_string(), line);
    }
}

fn do_give(rest: &str, ctx: &EvalCtx) {
    // `give <item-keyword> <player>`
    let (item_tok, player_tok) = split2(rest);
    if item_tok.is_empty() || player_tok.is_empty() {
        return;
    }
    let Some(item) = find_item_in_mob_inventory(&item_tok, ctx) else {
        return;
    };
    // Player target must be online and in the same room as the mob.
    if let Some(cid) = crate::session::find_player_connection_by_name(&ctx.connections, &player_tok) {
        // Resolve player name and room.
        let name_opt = {
            ctx.connections.lock().ok().and_then(|c| {
                c.get(&cid)
                    .and_then(|s| s.character.as_ref())
                    .map(|ch| (ch.name.clone(), ch.current_room_id))
            })
        };
        if let Some((player_name, player_room)) = name_opt {
            if Some(player_room) == ctx.self_room {
                let _ = ctx.db.move_item_to_inventory(&item.id, &player_name);
                let line = format!(
                    "{} gives you {}.\n",
                    capitalize_first(&ctx.self_name),
                    item.name
                );
                crate::send_client_message(&ctx.connections, cid.to_string(), line);
                let bcast = format!(
                    "{} gives {} to {}.",
                    capitalize_first(&ctx.self_name),
                    item.name,
                    player_name
                );
                broadcast_except(ctx, &bcast, Some(&player_name));
            }
        }
    }
}

fn do_drop(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(room_id) = ctx.self_room else {
        return;
    };
    let Some(item) = find_item_in_mob_inventory(item_tok, ctx) else {
        return;
    };
    let _ = ctx.db.move_item_to_room(&item.id, &room_id);
    let line = format!(
        "{} drops {}.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    broadcast(ctx, &line);
}

fn do_get(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(room_id) = ctx.self_room else {
        return;
    };
    let items = ctx.db.get_items_in_room(&room_id).unwrap_or_default();
    let lower = item_tok.to_ascii_lowercase();
    let Some(item) = items.into_iter().find(|i| {
        i.name.to_ascii_lowercase().contains(&lower)
            || i.keywords.iter().any(|k| k.to_ascii_lowercase().starts_with(&lower))
    }) else {
        return;
    };
    let _ = ctx.db.move_item_to_mobile_inventory(&item.id, &ctx.self_id);
    let line = format!(
        "{} picks up {}.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    broadcast(ctx, &line);
}

fn do_kill(rest: &str, ctx: &EvalCtx) {
    let target_tok = rest.trim();
    if target_tok.is_empty() {
        return;
    }
    let Some(target) = resolve_target_for_combat(target_tok, ctx) else {
        return;
    };
    let Ok(Some(mut mob)) = ctx.db.get_mobile_data(&ctx.self_id) else {
        return;
    };
    mob.combat.in_combat = true;
    let (target_type, target_id) = match target {
        ActorRef::Player { char_id, .. } => (crate::types::CombatTargetType::Player, char_id),
        ActorRef::Mob { mobile_id, .. } => (crate::types::CombatTargetType::Mobile, mobile_id),
    };
    if !mob.combat.targets.iter().any(|t| t.target_id == target_id) {
        mob.combat.targets.push(crate::types::CombatTarget {
            target_type,
            target_id,
            target_name: None,
        });
    }
    let _ = ctx.db.save_mobile_data(mob);
}

/// `wear <item>` / `wield <item>` — move the named item from the mob's
/// inventory to its equipped set. IronMUD has no slot-discrimination for
/// mob equipment (it's `Equipped(mob_id)` only), so wear and wield are
/// functionally identical here.
fn do_wear(rest: &str, ctx: &EvalCtx, fire_type: crate::types::ItemTriggerType) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(item) = find_item_in_mob_inventory(item_tok, ctx) else {
        return;
    };
    let _ = ctx.db.move_item_to_mobile_equipped(&item.id, &ctx.self_id);
    let line = format!(
        "{} equips {}.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    broadcast(ctx, &line);
    // Fire any equip-side DG triggers on the item itself (buff stamping is
    // automatic via the db layer). `fire_type` is OnWear for `wear` and
    // OnWield for `wield`.
    super::fire_item_dg_triggers(
        &ctx.db,
        &ctx.connections,
        &item,
        fire_type,
        "",
    );
}

/// `remove <item>` — move the named item from the mob's equipped set
/// back to its inventory.
fn do_remove(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let lower = item_tok.to_ascii_lowercase();
    let Ok(equipped) = ctx.db.get_items_equipped_on_mobile(&ctx.self_id) else {
        return;
    };
    let Some(item) = equipped.into_iter().find(|i| {
        i.name.to_ascii_lowercase().contains(&lower)
            || i.keywords
                .iter()
                .any(|k| k.to_ascii_lowercase().starts_with(&lower))
    }) else {
        return;
    };
    let _ = ctx.db.move_item_to_mobile_inventory(&item.id, &ctx.self_id);
    let line = format!(
        "{} removes {}.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    broadcast(ctx, &line);
    super::fire_item_dg_triggers(
        &ctx.db,
        &ctx.connections,
        &item,
        crate::types::ItemTriggerType::OnRemove,
        "",
    );
}

/// `quaff <potion>` — broadcast and consume the item from inventory. We
/// don't currently fire the potion's spell from a mob context (would need
/// a `cast_potion_on_mob` hook through the spell engine); stock tbamud
/// triggers using `quaff` are rare and primarily flavor.
fn do_quaff(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(item) = find_item_in_mob_inventory(item_tok, ctx) else {
        return;
    };
    let line = format!(
        "{} quaffs {}.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    broadcast(ctx, &line);
    let _ = ctx.db.delete_item(&item.id);
}

/// `junk <item>` — delete the named item from the mob's inventory.
/// Companion to `extract` (which deletes the mob itself).
fn do_junk(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(item) = find_item_in_mob_inventory(item_tok, ctx) else {
        return;
    };
    let _ = ctx.db.delete_item(&item.id);
}

/// `asound <text>` — broadcast a flat string to every room reachable from
/// the mob's room in one exit step (excluding self_room). Doesn't recurse,
/// doesn't follow doors. Stock uses for ambient distant sounds.
fn do_asound(rest: &str, ctx: &EvalCtx) {
    let mut text = rest.trim().to_string();
    if text.is_empty() {
        return;
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let Some(room_id) = ctx.self_room else { return };
    let Ok(Some(room)) = ctx.db.get_room_data(&room_id) else { return };
    let neighbors = [
        room.exits.north,
        room.exits.south,
        room.exits.east,
        room.exits.west,
        room.exits.up,
        room.exits.down,
    ];
    for n in neighbors.into_iter().flatten() {
        crate::broadcast_to_room(&ctx.connections, n, text.clone(), None);
    }
}

/// Direction-as-verb movement: walk the mob through the named exit.
fn do_walk(dir: &str, ctx: &EvalCtx) {
    let Some(room_id) = ctx.self_room else { return };
    let Ok(Some(room)) = ctx.db.get_room_data(&room_id) else { return };
    let dest = match dir {
        "north" => room.exits.north,
        "south" => room.exits.south,
        "east" => room.exits.east,
        "west" => room.exits.west,
        "up" => room.exits.up,
        "down" => room.exits.down,
        _ => None,
    };
    if let Some(dest) = dest {
        let _ = ctx.db.move_mobile_to_room(&ctx.self_id, &dest);
    }
}

fn do_door(action: &str, rest: &str, ctx: &EvalCtx) {
    // `<action> <direction>` — mutate the mob's current room's door in dir.
    let dir = rest.trim().to_ascii_lowercase();
    if dir.is_empty() {
        return;
    }
    let Some(room_id) = ctx.self_room else {
        return;
    };
    let canonical = canonical_dir(&dir);
    if canonical.is_empty() {
        return;
    }
    if let Ok(Some(mut room)) = ctx.db.get_room_data(&room_id) {
        if let Some(door) = room.doors.get_mut(&canonical) {
            match action {
                "open" => {
                    door.is_closed = false;
                    door.is_locked = false;
                }
                "close" => door.is_closed = true,
                "lock" => {
                    door.is_closed = true;
                    door.is_locked = true;
                }
                "unlock" => door.is_locked = false,
                _ => return,
            }
            let _ = ctx.db.save_room_data(room);
            let line = format!(
                "{} {}s the {} {}.",
                capitalize_first(&ctx.self_name),
                action,
                door_noun_or_default(action),
                canonical
            );
            broadcast(ctx, &line);
        }
    }
}

fn do_goto(rest: &str, ctx: &EvalCtx) {
    let dest_tok = rest.trim();
    if dest_tok.is_empty() {
        return;
    }
    let dest_id = match Uuid::parse_str(dest_tok) {
        Ok(u) => u,
        Err(_) => match ctx.db.get_room_by_vnum(dest_tok) {
            Ok(Some(r)) => r.id,
            _ => return,
        },
    };
    let _ = ctx.db.move_mobile_to_room(&ctx.self_id, &dest_id);
}

fn do_flee(ctx: &EvalCtx) {
    let Some(room_id) = ctx.self_room else {
        return;
    };
    let Ok(Some(room)) = ctx.db.get_room_data(&room_id) else {
        return;
    };
    let dirs = [
        room.exits.north,
        room.exits.south,
        room.exits.east,
        room.exits.west,
        room.exits.up,
        room.exits.down,
    ];
    let candidates: Vec<Uuid> = dirs.into_iter().flatten().collect();
    if candidates.is_empty() {
        return;
    }
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    if let Some(&dest) = candidates.choose(&mut rng) {
        let _ = ctx.db.move_mobile_to_room(&ctx.self_id, &dest);
    }
}

/// `list` — shopkeeper-mob broadcasts a "displays their wares" line
/// listing items from `shop_stock`. No-op for non-shopkeepers.
fn do_shop_list(ctx: &EvalCtx) {
    let Ok(Some(mob)) = ctx.db.get_mobile_data(&ctx.self_id) else { return };
    if !mob.flags.shopkeeper || mob.shop_stock.is_empty() {
        return;
    }
    let mut names: Vec<String> = Vec::new();
    for vnum in mob.shop_stock.iter().take(10) {
        if let Ok(Some(proto)) = ctx.db.get_item_by_vnum(vnum) {
            names.push(proto.name);
        }
    }
    if names.is_empty() {
        return;
    }
    let line = format!(
        "{} gestures at the wares: {}.",
        capitalize_first(&ctx.self_name),
        names.join(", ")
    );
    broadcast(ctx, &line);
}

/// `value <item>` — shopkeeper-mob broadcasts a price quote for the named
/// item from inventory or stock, computed via `shop_sell_rate`.
fn do_shop_value(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Ok(Some(mob)) = ctx.db.get_mobile_data(&ctx.self_id) else { return };
    if !mob.flags.shopkeeper {
        return;
    }
    // Look up the item — try mob's inventory first, then shop_stock prototypes.
    let lower = item_tok.to_ascii_lowercase();
    let inv_match = ctx
        .db
        .get_items_in_mobile_inventory(&ctx.self_id)
        .unwrap_or_default()
        .into_iter()
        .find(|i| {
            i.name.to_ascii_lowercase().contains(&lower)
                || i.keywords.iter().any(|k| k.to_ascii_lowercase().starts_with(&lower))
        });
    let stock_match = if inv_match.is_none() {
        mob.shop_stock.iter().find_map(|v| {
            ctx.db.get_item_by_vnum(v).ok().flatten().filter(|p| {
                p.name.to_ascii_lowercase().contains(&lower)
                    || p.keywords.iter().any(|k| k.to_ascii_lowercase().starts_with(&lower))
            })
        })
    } else {
        None
    };
    let Some(item) = inv_match.or(stock_match) else { return };
    let price = (item.value * mob.shop_sell_rate / 100).max(1);
    let line = format!(
        "{} appraises {}: \"That'll be {} gold.\"",
        capitalize_first(&ctx.self_name),
        item.name,
        price
    );
    broadcast(ctx, &line);
}

/// `light <item>` — toggle `provides_light` on a held torch/lamp.
fn do_light(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(mut item) = find_item_in_mob_inventory(item_tok, ctx) else { return };
    item.flags.provides_light = !item.flags.provides_light;
    let action = if item.flags.provides_light { "lights" } else { "snuffs" };
    let line = format!(
        "{} {} {}.",
        capitalize_first(&ctx.self_name),
        action,
        item.name
    );
    let _ = ctx.db.save_item_data(item);
    broadcast(ctx, &line);
}

/// `eat <item>` / `drink <item>` — consume the named food/drink from the
/// mob's inventory, broadcasting flavor and deleting the item.
fn do_consume(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(item) = find_item_in_mob_inventory(item_tok, ctx) else { return };
    let line = format!(
        "{} consumes {}.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    broadcast(ctx, &line);
    let _ = ctx.db.delete_item(&item.id);
}

/// `use <item>` — activate a held wand/staff. We don't currently route the
/// mob through the player cast pipeline, so this is a flavor broadcast and
/// a charge decrement when the item carries `charges`.
fn do_use(rest: &str, ctx: &EvalCtx) {
    let item_tok = rest.trim();
    if item_tok.is_empty() {
        return;
    }
    let Some(mut item) = find_item_in_mob_inventory(item_tok, ctx) else { return };
    let line = format!(
        "{} brandishes {} and triggers it.",
        capitalize_first(&ctx.self_name),
        item.name
    );
    if let Some(cou) = item.cast_on_use.as_mut() {
        if cou.charges > 0 {
            cou.charges -= 1;
            let _ = ctx.db.save_item_data(item);
        }
    }
    broadcast(ctx, &line);
}

/// `order <target> <command>` — proxy a verb to a charmed mob in the same
/// room. Mirrors the player `order` skill: only mobs whose `charm_master`
/// matches the self mob's name accept commands. The target is resolved by
/// keyword/name in the same room. The command is then re-dispatched
/// through `try_dispatch` with the target as self.
fn do_order(rest: &str, ctx: &EvalCtx) {
    let (target_tok, cmdline) = split2(rest);
    if target_tok.is_empty() || cmdline.is_empty() {
        return;
    }
    let Some(room_id) = ctx.self_room else { return };
    let Ok(mobs) = ctx.db.get_mobiles_in_room(&room_id) else { return };
    let lower = target_tok.to_ascii_lowercase();
    let self_name_lower = ctx.self_name.to_ascii_lowercase();
    let target = mobs.into_iter().find(|m| {
        m.id != ctx.self_id
            && m.charm_master()
                .map(|s| s.to_ascii_lowercase() == self_name_lower)
                .unwrap_or(false)
            && (m.name.to_ascii_lowercase().contains(&lower)
                || m.keywords.iter().any(|k| k.to_ascii_lowercase().starts_with(&lower)))
    });
    let Some(target) = target else { return };
    // Re-dispatch with the target as self. Keep the connections + db
    // shared; only swap the self-bindings.
    let sub_ctx = EvalCtx {
        db: ctx.db.clone(),
        connections: ctx.connections.clone(),
        self_kind: SelfKind::Mob,
        self_id: target.id,
        self_name: target.name.clone(),
        self_vnum: target.vnum.clone(),
        self_room: target.current_room_id,
        actor: ctx.actor.clone(),
        victim: ctx.victim.clone(),
        arg: ctx.arg.clone(),
        cmd: ctx.cmd.clone(),
        cmd_canonical: ctx.cmd_canonical.clone(),
        context_vars: ctx.context_vars.clone(),
        authored_by: ctx.authored_by.clone(),
        elevated: ctx.elevated,
        #[cfg(test)]
        test_temp_dir: None,
    };
    let (sub_verb, sub_rest) = split2(&cmdline);
    let _ = try_dispatch(&sub_verb, &sub_rest, &sub_ctx);
}

// ---------- helpers ----------

fn split2(s: &str) -> (String, String) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (s[..i].to_string(), s[i..].trim().to_string()),
        None => (s.to_string(), String::new()),
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

fn find_item_in_mob_inventory(keyword: &str, ctx: &EvalCtx) -> Option<crate::types::ItemData> {
    let lower = keyword.to_ascii_lowercase();
    let items = ctx.db.get_items_in_mobile_inventory(&ctx.self_id).ok()?;
    items.into_iter().find(|i| {
        i.name.to_ascii_lowercase().contains(&lower)
            || i.keywords
                .iter()
                .any(|k| k.to_ascii_lowercase().starts_with(&lower))
    })
}

fn resolve_target_for_combat(tok: &str, ctx: &EvalCtx) -> Option<ActorRef> {
    let tok = tok.trim();
    if tok.is_empty() {
        return None;
    }
    if let Ok(uid) = Uuid::parse_str(tok) {
        if let Ok(Some(mob)) = ctx.db.get_mobile_data(&uid) {
            return Some(ActorRef::Mob { mobile_id: uid, name: mob.name });
        }
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
        "self" => return None, // can't target self
        _ => {}
    }
    for cand in [&ctx.actor, &ctx.victim].into_iter().flatten() {
        if cand.name().eq_ignore_ascii_case(tok) {
            return Some(cand.clone());
        }
    }
    // Same-room mob match by keyword/name.
    if let Some(room_id) = ctx.self_room {
        if let Ok(mobs) = ctx.db.get_mobiles_in_room(&room_id) {
            let lower = tok.to_ascii_lowercase();
            for m in mobs {
                if m.id == ctx.self_id {
                    continue;
                }
                if m.name.to_ascii_lowercase().contains(&lower)
                    || m.keywords.iter().any(|k| k.to_ascii_lowercase().starts_with(&lower))
                {
                    return Some(ActorRef::Mob { mobile_id: m.id, name: m.name });
                }
            }
        }
    }
    None
}

fn canonical_dir(d: &str) -> String {
    match d {
        "n" | "north" => "north".to_string(),
        "s" | "south" => "south".to_string(),
        "e" | "east" => "east".to_string(),
        "w" | "west" => "west".to_string(),
        "u" | "up" => "up".to_string(),
        "d" | "down" => "down".to_string(),
        _ => String::new(),
    }
}

fn door_noun_or_default(_action: &str) -> &'static str {
    "door"
}

fn broadcast_except(ctx: &EvalCtx, text: &str, exclude_name: Option<&str>) {
    let Some(room_id) = ctx.self_room else {
        return;
    };
    let mut t = text.to_string();
    if !t.ends_with('\n') {
        t.push('\n');
    }
    crate::broadcast_to_room(&ctx.connections, room_id, t, exclude_name);
}

/// Stock-tbamud-style canned text for social emotes used by mob triggers.
/// Returns the broadcast string with `%self%` already substituted; `None`
/// when the verb isn't a recognised social.
fn social_emote_text(verb: &str, self_name: &str) -> Option<String> {
    let cap = capitalize_first(self_name);
    let s = match verb {
        "smile" => format!("{} smiles.", cap),
        "nod" => format!("{} nods.", cap),
        "grin" => format!("{} grins.", cap),
        "bow" => format!("{} bows deeply.", cap),
        "cry" => format!("{} bursts into tears.", cap),
        "wave" => format!("{} waves.", cap),
        "frown" => format!("{} frowns.", cap),
        "wink" => format!("{} winks.", cap),
        "shake" => format!("{} shakes {} head.", cap, "their"),
        "laugh" => format!("{} laughs.", cap),
        "growl" => format!("{} growls menacingly.", cap),
        "dance" => format!("{} starts to dance.", cap),
        "clap" => format!("{} claps loudly.", cap),
        "sigh" => format!("{} sighs.", cap),
        "poke" => format!("{} pokes you.", cap),
        "roll" => format!("{} rolls eyes.", cap),
        "hug" => format!("{} hugs you.", cap),
        "chuckle" => format!("{} chuckles.", cap),
        "yawn" => format!("{} yawns.", cap),
        "whisper" => format!("{} whispers something.", cap),
        "sing" => format!("{} hums a tune.", cap),
        "lick" => format!("{} licks {} lips.", cap, "their"),
        "kiss" => format!("{} blows a kiss.", cap),
        "cackle" => format!("{} cackles wildly.", cap),
        "smirk" => format!("{} smirks.", cap),
        "slap" => format!("{} slaps you.", cap),
        "snarl" => format!("{} snarls.", cap),
        "strut" => format!("{} struts proudly.", cap),
        "hum" => format!("{} hums softly.", cap),
        "ponder" => format!("{} ponders.", cap),
        "sniff" => format!("{} sniffs.", cap),
        "spit" => format!("{} spits.", cap),
        "scream" => format!("{} screams!", cap),
        // Phase 8f socials — common stock-tbamud emote vocabulary.
        "shout" => format!("{} shouts.", cap),
        "beam" => format!("{} beams a smile.", cap),
        "em" => format!("{} emotes.", cap),
        "flex" => format!("{} flexes muscles.", cap),
        "giggle" => format!("{} giggles.", cap),
        "glare" => format!("{} glares.", cap),
        "grumble" => format!("{} grumbles.", cap),
        "mumble" => format!("{} mumbles.", cap),
        "peer" => format!("{} peers around intently.", cap),
        "pout" => format!("{} pouts.", cap),
        "ogle" => format!("{} ogles.", cap),
        "ruffle" => format!("{} ruffles {} hair.", cap, "their"),
        "drool" => format!("{} drools.", cap),
        "shiver" => format!("{} shivers.", cap),
        "sneeze" => format!("{} sneezes.", cap),
        "hiss" => format!("{} hisses.", cap),
        "grimace" => format!("{} grimaces.", cap),
        "eyebrow" => format!("{} raises an eyebrow.", cap),
        "gaze" => format!("{} gazes.", cap),
        "caress" => format!("{} caresses you.", cap),
        "pet" => format!("{} pets you.", cap),
        "bounce" => format!("{} bounces around.", cap),
        "squeeze" => format!("{} squeezes you.", cap),
        "think" => format!("{} thinks.", cap),
        "welcome" => format!("{} welcomes you.", cap),
        "great" => format!("{} greets you warmly.", cap),
        "mua" | "muah" => format!("{} blows a kiss.", cap),
        "play" => format!("{} plays around.", cap),
        "sac" | "sacrifice" => format!("{} sacrifices to the gods.", cap),
        "rem" => format!("{} reminisces.", cap),
        _ => return None,
    };
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CombatState, ItemData, ItemLocation, MobileData, RoomData};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn make_ctx(room_id: Uuid, mob: &MobileData, _label: &str) -> EvalCtx {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().to_owned();
        let db = Arc::new(crate::db::Db::open(&path).expect("open db"));
        let connections: crate::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        EvalCtx {
            db,
            connections,
            self_kind: SelfKind::Mob,
            self_id: mob.id,
            self_name: mob.name.clone(),
            self_vnum: mob.vnum.clone(),
            self_room: Some(room_id),
            actor: None,
            victim: None,
            arg: String::new(),
            cmd: String::new(),
            cmd_canonical: String::new(),
            context_vars: HashMap::new(),
            authored_by: None,
            elevated: false,
            test_temp_dir: Some(Arc::new(temp)),
        }
    }

    fn build_room(room_id: Uuid) -> RoomData {
        serde_json::from_value(serde_json::json!({
            "id": room_id,
            "vnum": null,
            "title": "test room",
            "description": "",
            "exits": {},
            "doors": {},
            "items": [],
            "extra_descs": [],
            "triggers": [],
            "flags": {},
            "area_id": null,
        }))
        .expect("build room")
    }

    fn save_room(ctx: &EvalCtx, room_id: Uuid) {
        ctx.db.save_room_data(build_room(room_id)).expect("save room");
    }

    fn build_item(name: &str, owner_mob_id: Uuid) -> ItemData {
        let mut item = ItemData::new(name.to_string(), String::new(), String::new());
        item.keywords = vec![name.to_string()];
        item.location = ItemLocation::Inventory(owner_mob_id.to_string());
        item
    }

    #[test]
    fn try_dispatch_returns_false_for_non_mob_self() {
        let mob = MobileData::new("guard".to_string());
        let mut ctx = make_ctx(Uuid::new_v4(), &mob, "non_mob_self");
        ctx.self_kind = SelfKind::Obj;
        assert!(!try_dispatch("say", "hello", &ctx));
    }

    #[test]
    fn say_is_dispatched() {
        let mob = MobileData::new("guard".to_string());
        let room_id = Uuid::new_v4();
        let ctx = make_ctx(room_id, &mob, "say_dispatched");
        save_room(&ctx, room_id);
        assert!(try_dispatch("say", "hi there", &ctx));
        // No connection in room → broadcast just no-ops, but verb is recognised.
    }

    #[test]
    fn drop_moves_item_from_mob_to_room() {
        let mut mob = MobileData::new("guard".to_string());
        mob.is_prototype = false;
        let room_id = Uuid::new_v4();
        mob.current_room_id = Some(room_id);
        let mob_id = mob.id;
        let ctx = make_ctx(room_id, &mob, "drop");
        save_room(&ctx, room_id);
        ctx.db.save_mobile_data(mob).expect("save mob");

        let item = build_item("rock", mob_id);
        let item_id = item.id;
        ctx.db.save_item_data(item).expect("save item");

        assert!(try_dispatch("drop", "rock", &ctx));
        let after = ctx.db.get_item_data(&item_id).unwrap().unwrap();
        assert!(matches!(after.location, ItemLocation::Room(rid) if rid == room_id));
    }

    #[test]
    fn kill_initiates_combat() {
        let mut attacker = MobileData::new("attacker".to_string());
        attacker.is_prototype = false;
        let room_id = Uuid::new_v4();
        attacker.current_room_id = Some(room_id);
        let mut foe = MobileData::new("rabid wolf".to_string());
        foe.is_prototype = false;
        foe.keywords = vec!["wolf".to_string()];
        foe.current_room_id = Some(room_id);
        let attacker_id = attacker.id;
        let foe_id = foe.id;

        let ctx = make_ctx(room_id, &attacker, "kill");
        save_room(&ctx, room_id);
        ctx.db.save_mobile_data(attacker).expect("save attacker");
        ctx.db.save_mobile_data(foe).expect("save foe");

        assert!(try_dispatch("kill", "wolf", &ctx));

        let after = ctx.db.get_mobile_data(&attacker_id).unwrap().unwrap();
        assert!(after.combat.in_combat);
        assert!(after.combat.targets.iter().any(|t| t.target_id == foe_id));
    }

    #[test]
    fn close_marks_door_closed() {
        let mut mob = MobileData::new("guard".to_string());
        mob.is_prototype = false;
        let room_id = Uuid::new_v4();
        mob.current_room_id = Some(room_id);
        let ctx = make_ctx(room_id, &mob, "close_door");

        let mut room = build_room(room_id);
        room.doors.insert(
            "north".to_string(),
            crate::types::DoorState {
                name: "door".to_string(),
                is_closed: false,
                is_locked: false,
                ..Default::default()
            },
        );
        ctx.db.save_room_data(room).expect("save room");
        ctx.db.save_mobile_data(mob).expect("save mob");

        assert!(try_dispatch("close", "n", &ctx));
        let after = ctx.db.get_room_data(&room_id).unwrap().unwrap();
        assert!(after.doors.get("north").unwrap().is_closed);
    }

    #[test]
    fn goto_moves_mobile() {
        let mut mob = MobileData::new("guard".to_string());
        mob.is_prototype = false;
        let src = Uuid::new_v4();
        let dst = Uuid::new_v4();
        mob.current_room_id = Some(src);
        let mob_id = mob.id;
        let ctx = make_ctx(src, &mob, "goto");
        save_room(&ctx, src);
        ctx.db.save_room_data(build_room(dst)).expect("save dst");
        ctx.db.save_mobile_data(mob).expect("save mob");

        let arg = dst.to_string();
        assert!(try_dispatch("goto", &arg, &ctx));
        let after = ctx.db.get_mobile_data(&mob_id).unwrap().unwrap();
        assert_eq!(after.current_room_id, Some(dst));
    }

    #[test]
    fn social_emote_returns_canned_text() {
        assert!(social_emote_text("smile", "guard").unwrap().contains("smiles"));
        assert!(social_emote_text("bow", "guard").unwrap().contains("bows"));
        assert!(social_emote_text("nonsense", "guard").is_none());
    }

    #[test]
    fn wear_moves_item_from_inventory_to_equipped() {
        let mut mob = MobileData::new("guard".to_string());
        mob.is_prototype = false;
        let room_id = Uuid::new_v4();
        mob.current_room_id = Some(room_id);
        let mob_id = mob.id;
        let ctx = make_ctx(room_id, &mob, "wear");
        save_room(&ctx, room_id);
        ctx.db.save_mobile_data(mob).expect("save mob");

        let item = build_item("shield", mob_id);
        let item_id = item.id;
        ctx.db.save_item_data(item).expect("save item");

        assert!(try_dispatch("wear", "shield", &ctx));
        let after = ctx.db.get_item_data(&item_id).unwrap().unwrap();
        assert!(matches!(after.location, ItemLocation::Equipped(ref s) if s == &mob_id.to_string()));
    }

    #[test]
    fn remove_moves_item_from_equipped_to_inventory() {
        let mut mob = MobileData::new("guard".to_string());
        mob.is_prototype = false;
        let room_id = Uuid::new_v4();
        mob.current_room_id = Some(room_id);
        let mob_id = mob.id;
        let ctx = make_ctx(room_id, &mob, "remove");
        save_room(&ctx, room_id);
        ctx.db.save_mobile_data(mob).expect("save mob");

        let mut item = build_item("shield", mob_id);
        item.location = ItemLocation::Equipped(mob_id.to_string());
        let item_id = item.id;
        ctx.db.save_item_data(item).expect("save item");

        assert!(try_dispatch("remove", "shield", &ctx));
        let after = ctx.db.get_item_data(&item_id).unwrap().unwrap();
        assert!(matches!(after.location, ItemLocation::Inventory(ref s) if s == &mob_id.to_string()));
    }

    #[test]
    fn quaff_consumes_potion_from_inventory() {
        let mut mob = MobileData::new("guard".to_string());
        mob.is_prototype = false;
        let room_id = Uuid::new_v4();
        mob.current_room_id = Some(room_id);
        let mob_id = mob.id;
        let ctx = make_ctx(room_id, &mob, "quaff");
        save_room(&ctx, room_id);
        ctx.db.save_mobile_data(mob).expect("save mob");

        let item = build_item("potion", mob_id);
        let item_id = item.id;
        ctx.db.save_item_data(item).expect("save item");

        assert!(try_dispatch("quaff", "potion", &ctx));
        let after = ctx.db.get_item_data(&item_id).unwrap();
        assert!(after.is_none(), "expected potion deleted, got {:?}", after);
    }

    #[test]
    fn unknown_verb_returns_false() {
        let mob = MobileData::new("guard".to_string());
        let ctx = make_ctx(Uuid::new_v4(), &mob, "unknown");
        assert!(!try_dispatch("xyzzy", "", &ctx));
    }

    // Force the unused imports to count.
    #[allow(dead_code)]
    fn _unused_imports_marker() -> CombatState {
        CombatState::default()
    }
}

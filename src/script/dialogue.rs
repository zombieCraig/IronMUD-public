//! Dialogue tree engine: branching mob dialogue with conditions and effects.
//!
//! Trees live on `MobileData.dialogue_tree`. Player conversation cursor is
//! per-(player, mob-vnum) on `CharacterData.dialogue_pair_state`. Per-pair
//! flags ride on `CharacterData.dialogue_flags` keyed by `<vnum>:<name>`
//! (Local scope) or `<name>` (Global scope).
//!
//! Two UX paths share this engine:
//! - `talk <mob>` enters sticky mode (tracked via
//!   `PlayerSession.dialogue_partner_id`); numeric choices route through
//!   `walk_dialogue_choice`.
//! - `say <keyword>` walks the same tree opportunistically without
//!   sticky mode via `walk_dialogue_keyword`.
//!
//! Falls through to flat `MobileData.dialogue` HashMap on tree miss so
//! existing keyword-only mobs keep working.

use std::sync::Arc;

use rhai::{Engine, Map};
use uuid::Uuid;

use crate::{SharedConnections, SharedState};
use crate::db::Db;
use crate::types::{
    CharacterData, DgScope, DialogueChoice, DialogueCondition, DialogueEffect, DialogueNode,
    DialoguePairState, DialogueTarget, DialogueTree, FlagScope, MobileData, MobileTriggerType,
};

// ===== Public Rhai-callable API =====

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // state is plumbed through apply_effect so the CompleteQuest dialogue effect
    // can call crate::quest::try_complete (which needs SharedState to grant
    // Achievement rewards via crate::script::achievements::award_core).
    // is_in_dialogue(connection_id) -> bool
    {
        let conns = connections.clone();
        engine.register_fn("is_in_dialogue", move |connection_id: String| -> bool {
            session_dialogue_partner(&conns, &connection_id).is_some()
        });
    }

    // get_dialogue_partner_id(connection_id) -> String (uuid; empty if none)
    {
        let conns = connections.clone();
        engine.register_fn("get_dialogue_partner_id", move |connection_id: String| -> String {
            session_dialogue_partner(&conns, &connection_id)
                .map(|u| u.to_string())
                .unwrap_or_default()
        });
    }

    // start_talk(connection_id, mob_keyword) -> Map
    //
    // Looks up a mob in the player's room by keyword and enters sticky
    // dialogue mode. Returns:
    //   { ok: bool, error: String,
    //     mob_id, mob_short_desc, mob_room_id, response, menu, finished }
    {
        let cloned_db = db.clone();
        let conns = connections.clone();
        let st = state.clone();
        engine.register_fn(
            "start_talk",
            move |connection_id: String, mob_keyword: String| -> Map {
                let mut out = empty_result();
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return err(out, "invalid connection id"),
                };
                let ch = match get_character_for_conn(&conns, conn_uuid) {
                    Some(c) => c,
                    None => return err(out, "not logged in"),
                };
                let room_id = match ch.current_room_id {
                    rid if rid != Uuid::nil() => rid,
                    _ => return err(out, "no current room"),
                };
                // Find mobs in room with a tree, match keyword.
                let mob = match find_mob_in_room_with_tree(&cloned_db, room_id, &mob_keyword) {
                    Some(m) => m,
                    None => return err(out, "no such mob with dialogue here"),
                };
                set_session_dialogue_partner(&conns, conn_uuid, Some(mob.id));
                let mut ch_mut = ch;
                let view = enter_root_or_keep(&cloned_db, &conns, &st, &mut ch_mut, &mob);
                save_character_and_sync(&cloned_db, &conns, conn_uuid, ch_mut);
                out.insert("ok".into(), rhai::Dynamic::from(true));
                out.insert("mob_id".into(), rhai::Dynamic::from(mob.id.to_string()));
                out.insert(
                    "mob_short_desc".into(),
                    rhai::Dynamic::from(mob.short_desc.clone()),
                );
                out.insert(
                    "mob_room_id".into(),
                    rhai::Dynamic::from(mob.current_room_id.unwrap_or(Uuid::nil()).to_string()),
                );
                out.insert("response".into(), rhai::Dynamic::from(view.response));
                out.insert("menu".into(), rhai::Dynamic::from(view.menu));
                out.insert("finished".into(), rhai::Dynamic::from(view.finished));
                out
            },
        );
    }

    // walk_dialogue_keyword(connection_id, keyword) -> Map (same shape).
    //
    // Search order: active sticky partner first; otherwise iterate same-room
    // mobs with trees and pick the first whose current node has a visible
    // matching choice. Falls through with ok=false if no match — caller
    // (say.rhai) then runs the flat dialogue check.
    {
        let cloned_db = db.clone();
        let conns = connections.clone();
        let st = state.clone();
        engine.register_fn(
            "walk_dialogue_keyword",
            move |connection_id: String, keyword: String| -> Map {
                let mut out = empty_result();
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return err(out, "invalid connection id"),
                };
                let ch = match get_character_for_conn(&conns, conn_uuid) {
                    Some(c) => c,
                    None => return err(out, "not logged in"),
                };
                let room_id = ch.current_room_id;
                if room_id == Uuid::nil() {
                    return err(out, "no current room");
                }
                let active_partner = session_dialogue_partner(&conns, &connection_id);
                let candidates: Vec<MobileData> = if let Some(pid) = active_partner {
                    cloned_db
                        .get_mobile_data(&pid)
                        .ok()
                        .flatten()
                        .filter(|m| m.dialogue_tree.is_some() && m.current_room_id == Some(room_id))
                        .into_iter()
                        .collect()
                } else {
                    list_room_mobs_with_trees(&cloned_db, room_id)
                };
                let needle = keyword.trim().to_lowercase();
                if needle.is_empty() {
                    return err(out, "empty keyword");
                }
                for mob in candidates {
                    let mut ch_mut = ch.clone();
                    let cur_node = current_node_for(&ch_mut, &mob);
                    let tree = mob.dialogue_tree.as_ref().unwrap();
                    let node = match tree.nodes.get(&cur_node) {
                        Some(n) => n,
                        None => continue,
                    };
                    let classified = classify_choices(
                        &cur_node,
                        node,
                        &ch_mut,
                        &mob,
                        &cloned_db,
                        now_epoch_secs(),
                    );
                    let entry = classified
                        .iter()
                        .find(|e| e.choice.keyword.eq_ignore_ascii_case(&needle));
                    let Some(entry) = entry else {
                        continue;
                    };
                    let (response, menu, finished) = match &entry.visibility {
                        ChoiceVisibility::Available => take_choice_at_node(
                            &cloned_db,
                            &conns,
                            &st,
                            &mut ch_mut,
                            &mob,
                            &cur_node,
                            &entry.choice,
                        ),
                        ChoiceVisibility::Locked { .. } => (
                            "That doesn't seem available right now.".to_string(),
                            render_classified_menu(&classified),
                            false,
                        ),
                        ChoiceVisibility::Cooldown { .. } => (
                            "You'll need to wait before raising that again.".to_string(),
                            render_classified_menu(&classified),
                            false,
                        ),
                    };
                    save_character_and_sync(&cloned_db, &conns, conn_uuid, ch_mut);
                    if finished {
                        set_session_dialogue_partner(&conns, conn_uuid, None);
                    }
                    out.insert("ok".into(), rhai::Dynamic::from(true));
                    out.insert("mob_id".into(), rhai::Dynamic::from(mob.id.to_string()));
                    out.insert(
                        "mob_short_desc".into(),
                        rhai::Dynamic::from(mob.short_desc.clone()),
                    );
                    out.insert(
                        "mob_room_id".into(),
                        rhai::Dynamic::from(
                            mob.current_room_id.unwrap_or(Uuid::nil()).to_string(),
                        ),
                    );
                    out.insert("response".into(), rhai::Dynamic::from(response));
                    out.insert("menu".into(), rhai::Dynamic::from(menu));
                    out.insert("finished".into(), rhai::Dynamic::from(finished));
                    return out;
                }
                out.insert("ok".into(), rhai::Dynamic::from(false));
                out
            },
        );
    }

    // walk_dialogue_choice(connection_id, idx) -> Map (1-indexed; sticky mode only)
    {
        let cloned_db = db.clone();
        let conns = connections.clone();
        let st = state.clone();
        engine.register_fn(
            "walk_dialogue_choice",
            move |connection_id: String, idx: i64| -> Map {
                let mut out = empty_result();
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return err(out, "invalid connection id"),
                };
                let ch = match get_character_for_conn(&conns, conn_uuid) {
                    Some(c) => c,
                    None => return err(out, "not logged in"),
                };
                let partner_id = match session_dialogue_partner(&conns, &connection_id) {
                    Some(p) => p,
                    None => return err(out, "not in a dialogue"),
                };
                let mob = match cloned_db.get_mobile_data(&partner_id).ok().flatten() {
                    Some(m) if m.dialogue_tree.is_some() => m,
                    _ => return err(out, "partner gone"),
                };
                let mut ch_mut = ch;
                let cur_node = current_node_for(&ch_mut, &mob);
                let tree = mob.dialogue_tree.as_ref().unwrap();
                let node = match tree.nodes.get(&cur_node) {
                    Some(n) => n,
                    None => return err(out, "node missing"),
                };
                let classified = classify_choices(
                    &cur_node,
                    node,
                    &ch_mut,
                    &mob,
                    &cloned_db,
                    now_epoch_secs(),
                );
                if idx < 1 || (idx as usize) > classified.len() {
                    return err(out, "invalid choice");
                }
                let entry = classified[(idx as usize) - 1].clone();
                let (response, menu, finished) = match &entry.visibility {
                    ChoiceVisibility::Available => take_choice_at_node(
                        &cloned_db,
                        &conns,
                        &st,
                        &mut ch_mut,
                        &mob,
                        &cur_node,
                        &entry.choice,
                    ),
                    ChoiceVisibility::Locked { .. } => (
                        "That doesn't seem available right now.".to_string(),
                        render_classified_menu(&classified),
                        false,
                    ),
                    ChoiceVisibility::Cooldown { .. } => (
                        "You'll need to wait before raising that again.".to_string(),
                        render_classified_menu(&classified),
                        false,
                    ),
                };
                save_character_and_sync(&cloned_db, &conns, conn_uuid, ch_mut);
                if finished {
                    set_session_dialogue_partner(&conns, conn_uuid, None);
                }
                out.insert("ok".into(), rhai::Dynamic::from(true));
                out.insert("mob_id".into(), rhai::Dynamic::from(mob.id.to_string()));
                out.insert(
                    "mob_short_desc".into(),
                    rhai::Dynamic::from(mob.short_desc.clone()),
                );
                out.insert(
                    "mob_room_id".into(),
                    rhai::Dynamic::from(mob.current_room_id.unwrap_or(Uuid::nil()).to_string()),
                );
                out.insert("response".into(), rhai::Dynamic::from(response));
                out.insert("menu".into(), rhai::Dynamic::from(menu));
                out.insert("finished".into(), rhai::Dynamic::from(finished));
                out
            },
        );
    }

    // exit_dialogue(connection_id) -> Map { ok, mob_id, mob_short_desc, mob_room_id }
    {
        let cloned_db = db.clone();
        let conns = connections.clone();
        let st = state.clone();
        engine.register_fn("exit_dialogue", move |connection_id: String| -> Map {
            let mut out = empty_result();
            let conn_uuid = match Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return out,
            };
            let partner_id = match session_dialogue_partner(&conns, &connection_id) {
                Some(p) => p,
                None => return out,
            };
            set_session_dialogue_partner(&conns, conn_uuid, None);
            // Clear the cursor for this partner-vnum so a fresh `talk` starts at root.
            let mut exit_msg = String::new();
            if let Some(mob) = cloned_db.get_mobile_data(&partner_id).ok().flatten() {
                if let Some(mut ch) = get_character_for_conn(&conns, conn_uuid) {
                    let cur = current_node_for(&ch, &mob);
                    exit_msg = exit_node(&cloned_db, &conns, &st, &mut ch, &mob, &cur);
                    if let Some(state) = ch.dialogue_pair_state.get_mut(&mob.vnum) {
                        state.current_node = None;
                    }
                    save_character_and_sync(&cloned_db, &conns, conn_uuid, ch);
                }
                out.insert("ok".into(), rhai::Dynamic::from(true));
                out.insert("mob_id".into(), rhai::Dynamic::from(mob.id.to_string()));
                out.insert(
                    "mob_short_desc".into(),
                    rhai::Dynamic::from(mob.short_desc.clone()),
                );
                out.insert(
                    "mob_room_id".into(),
                    rhai::Dynamic::from(mob.current_room_id.unwrap_or(Uuid::nil()).to_string()),
                );
                out.insert("exit_message".into(), rhai::Dynamic::from(exit_msg));
            }
            out
        });
    }

    // render_dialogue_menu(connection_id) -> String (numbered menu only, no mob text)
    {
        let cloned_db = db.clone();
        let conns = connections.clone();
        engine.register_fn(
            "render_dialogue_menu",
            move |connection_id: String| -> String {
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return String::new(),
                };
                let Some(partner_id) = session_dialogue_partner(&conns, &connection_id) else {
                    return String::new();
                };
                let Some(ch) = get_character_for_conn(&conns, conn_uuid) else {
                    return String::new();
                };
                let Some(mob) = cloned_db.get_mobile_data(&partner_id).ok().flatten() else {
                    return String::new();
                };
                let cur = current_node_for(&ch, &mob);
                let Some(tree) = mob.dialogue_tree.as_ref() else {
                    return String::new();
                };
                let Some(node) = tree.nodes.get(&cur) else {
                    return String::new();
                };
                render_classified_menu(&classify_choices(
                    &cur,
                    node,
                    &ch,
                    &mob,
                    &cloned_db,
                    now_epoch_secs(),
                ))
            },
        );
    }

    // set_mobile_dialogue_tree_json(mobile_id, json) -> String (empty=ok, error msg otherwise).
    // Pass empty string to clear.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "set_mobile_dialogue_tree_json",
            move |mobile_id: String, json: String| -> String {
                let mob_uuid = match Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return "invalid mobile id".to_string(),
                };
                let mut mob = match cloned_db.get_mobile_data(&mob_uuid).ok().flatten() {
                    Some(m) => m,
                    None => return "mobile not found".to_string(),
                };
                if json.trim().is_empty() {
                    mob.dialogue_tree = None;
                } else {
                    match serde_json::from_str::<DialogueTree>(&json) {
                        Ok(tree) => {
                            if let Err(e) = validate_tree(&tree) {
                                return e;
                            }
                            mob.dialogue_tree = Some(tree);
                        }
                        Err(e) => return format!("parse error: {}", e),
                    }
                }
                if let Err(e) = cloned_db.save_mobile_data(mob) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // get_mobile_dialogue_tree_json(mobile_id) -> String (pretty-printed; empty if no tree)
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "get_mobile_dialogue_tree_json",
            move |mobile_id: String| -> String {
                let mob_uuid = match Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return String::new(),
                };
                let mob = match cloned_db.get_mobile_data(&mob_uuid).ok().flatten() {
                    Some(m) => m,
                    None => return String::new(),
                };
                match mob.dialogue_tree {
                    Some(t) => serde_json::to_string_pretty(&t).unwrap_or_default(),
                    None => String::new(),
                }
            },
        );
    }

    // clear_mobile_dialogue_tree(mobile_id) -> bool
    {
        let cloned_db = db.clone();
        engine.register_fn("clear_mobile_dialogue_tree", move |mobile_id: String| -> bool {
            let mob_uuid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mob = match cloned_db.get_mobile_data(&mob_uuid).ok().flatten() {
                Some(m) => m,
                None => return false,
            };
            mob.dialogue_tree = None;
            cloned_db.save_mobile_data(mob).is_ok()
        });
    }

    // has_mobile_dialogue_tree(mobile_id) -> bool
    {
        let cloned_db = db.clone();
        engine.register_fn("has_mobile_dialogue_tree", move |mobile_id: String| -> bool {
            let mob_uuid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db
                .get_mobile_data(&mob_uuid)
                .ok()
                .flatten()
                .map(|m| m.dialogue_tree.is_some())
                .unwrap_or(false)
        });
    }

    // ----- Granular OLC bindings (mirror src/dialogue_edit.rs operations) ---
    // All return "" on success or an error message; pair with builder UX in
    // scripts/lib/dialogue_olc.rhai.

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_set_dialogue_root",
            move |mobile_id: String, node_name: String| -> String {
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::set_root(slot, &node_name)
                })
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_add_dialogue_node",
            move |mobile_id: String, name: String, text: String| -> String {
                let cloned_db = cloned_db.clone();
                let mob_uuid = match Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return "invalid mobile id".to_string(),
                };
                let mut mob = match cloned_db.get_mobile_data(&mob_uuid).ok().flatten() {
                    Some(m) => m,
                    None => return "mobile not found".to_string(),
                };
                // Auto-init: first node added becomes root.
                if mob.dialogue_tree.is_none() {
                    crate::dialogue_edit::ensure_initialized(&mut mob.dialogue_tree, &text);
                    if name != "root" {
                        if let Some(t) = mob.dialogue_tree.as_mut() {
                            if let Some(n) = t.nodes.remove("root") {
                                t.nodes.insert(name.clone(), n);
                                t.root_node = name;
                            }
                        }
                    }
                } else {
                    let node = DialogueNode {
                        text,
                        choices: vec![],
                        on_enter: vec![],
                        on_each_visit: vec![],
                        on_exit: vec![],
                    };
                    if let Err(e) =
                        crate::dialogue_edit::add_node(&mut mob.dialogue_tree, &name, node)
                    {
                        return e.to_string();
                    }
                }
                if let Err(e) = cloned_db.save_mobile_data(mob) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_remove_dialogue_node",
            move |mobile_id: String, name: String| -> String {
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::remove_node(slot, &name)
                })
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_set_dialogue_node_text",
            move |mobile_id: String, name: String, text: String| -> String {
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::update_node(
                        slot,
                        &name,
                        crate::dialogue_edit::NodePatch {
                            text: Some(text),
                            on_enter: None,
                            on_each_visit: None,
                            on_exit: None,
                        },
                    )
                })
            },
        );
    }

    // olc_add_dialogue_choice(mobile_id, node_name, keyword, label, target_kind, target_node)
    //   target_kind: "goto" | "exit" | "repeat"
    //   target_node: ignored unless target_kind=="goto"
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_add_dialogue_choice",
            move |mobile_id: String,
                  node_name: String,
                  keyword: String,
                  label: String,
                  target_kind: String,
                  target_node: String|
                  -> String {
                let target = match parse_target(&target_kind, &target_node) {
                    Ok(t) => t,
                    Err(e) => return e,
                };
                let choice = DialogueChoice {
                    keyword,
                    label,
                    target,
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                };
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::add_choice(slot, &node_name, choice)
                })
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_remove_dialogue_choice",
            move |mobile_id: String, node_name: String, index: i64| -> String {
                if index < 0 {
                    return "index must be >= 0".to_string();
                }
                let idx = index as usize;
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::remove_choice(slot, &node_name, idx)
                })
            },
        );
    }

    // olc_set_dialogue_choice_field(mobile_id, node_name, index, field, value)
    //   field: "hint" | "cooldown" | "once"
    //   value: free-form string. Empty hint clears; cooldown=="0" clears; once "on"|"off".
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_set_dialogue_choice_field",
            move |mobile_id: String,
                  node_name: String,
                  index: i64,
                  field: String,
                  value: String|
                  -> String {
                if index < 0 {
                    return "index must be >= 0".to_string();
                }
                let idx = index as usize;
                let field_lc = field.to_lowercase();
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    let tree = match slot.as_mut() {
                        Some(t) => t,
                        None => return Err(crate::dialogue_edit::DialogueEditError::NoTree),
                    };
                    let node = match tree.nodes.get_mut(&node_name) {
                        Some(n) => n,
                        None => {
                            return Err(crate::dialogue_edit::DialogueEditError::NodeMissing(
                                node_name.clone(),
                            ))
                        }
                    };
                    if idx >= node.choices.len() {
                        return Err(
                            crate::dialogue_edit::DialogueEditError::ChoiceIndexOutOfRange(
                                idx,
                                node.choices.len(),
                            ),
                        );
                    }
                    let choice = &mut node.choices[idx];
                    match field_lc.as_str() {
                        "hint" => {
                            choice.hint = if value.is_empty() {
                                None
                            } else {
                                Some(value.clone())
                            };
                        }
                        "cooldown" | "cooldown_secs" => {
                            let secs: i64 = value.trim().parse().unwrap_or(-1);
                            if secs < 0 {
                                return Err(crate::dialogue_edit::DialogueEditError::Invalid(
                                    "cooldown must be a non-negative integer".into(),
                                ));
                            }
                            choice.cooldown_secs = if secs == 0 { None } else { Some(secs) };
                        }
                        "once" | "once_per_player" => {
                            let v = value.trim().to_lowercase();
                            choice.once_per_player =
                                matches!(v.as_str(), "1" | "on" | "true" | "yes");
                        }
                        other => {
                            return Err(crate::dialogue_edit::DialogueEditError::Invalid(format!(
                                "unknown field `{}` (use hint|cooldown|once)",
                                other
                            )));
                        }
                    }
                    Ok(())
                })
            },
        );
    }

    // olc_add_choice_condition(mobile_id, node, choice_idx, kind, args)
    //   args is a free-form space-separated string parsed per kind.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_add_choice_condition",
            move |mobile_id: String,
                  node_name: String,
                  choice_index: i64,
                  kind: String,
                  args: String|
                  -> String {
                if choice_index < 0 {
                    return "choice index must be >= 0".to_string();
                }
                let condition = match parse_choice_condition(&kind, &args) {
                    Ok(c) => c,
                    Err(msg) => return msg,
                };
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::add_choice_condition(
                        slot,
                        &node_name,
                        choice_index as usize,
                        condition,
                    )
                })
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_remove_choice_condition",
            move |mobile_id: String,
                  node_name: String,
                  choice_index: i64,
                  cond_index: i64|
                  -> String {
                if choice_index < 0 || cond_index < 0 {
                    return "indices must be >= 0".to_string();
                }
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::remove_choice_condition(
                        slot,
                        &node_name,
                        choice_index as usize,
                        cond_index as usize,
                    )
                })
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_add_choice_effect",
            move |mobile_id: String,
                  node_name: String,
                  choice_index: i64,
                  kind: String,
                  args: String|
                  -> String {
                if choice_index < 0 {
                    return "choice index must be >= 0".to_string();
                }
                let effect = match parse_choice_effect(&kind, &args) {
                    Ok(e) => e,
                    Err(msg) => return msg,
                };
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::add_choice_effect(
                        slot,
                        &node_name,
                        choice_index as usize,
                        effect,
                    )
                })
            },
        );
    }

    {
        let cloned_db = db.clone();
        engine.register_fn(
            "olc_remove_choice_effect",
            move |mobile_id: String,
                  node_name: String,
                  choice_index: i64,
                  effect_index: i64|
                  -> String {
                if choice_index < 0 || effect_index < 0 {
                    return "indices must be >= 0".to_string();
                }
                mutate_tree(&cloned_db, &mobile_id, |slot| {
                    crate::dialogue_edit::remove_choice_effect(
                        slot,
                        &node_name,
                        choice_index as usize,
                        effect_index as usize,
                    )
                })
            },
        );
    }

    // List node names as a comma-joined string for medit display.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "list_dialogue_node_names",
            move |mobile_id: String| -> String {
                let mob_uuid = match Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return String::new(),
                };
                let mob = match cloned_db.get_mobile_data(&mob_uuid).ok().flatten() {
                    Some(m) => m,
                    None => return String::new(),
                };
                let Some(tree) = mob.dialogue_tree else {
                    return String::new();
                };
                let mut names: Vec<&String> = tree.nodes.keys().collect();
                names.sort();
                names.into_iter().cloned().collect::<Vec<_>>().join(", ")
            },
        );
    }

    // Render one node for medit: text + numbered choice list with target tags.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "describe_dialogue_node",
            move |mobile_id: String, node_name: String| -> String {
                let mob_uuid = match Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return "invalid mobile id".to_string(),
                };
                let mob = match cloned_db.get_mobile_data(&mob_uuid).ok().flatten() {
                    Some(m) => m,
                    None => return "mobile not found".to_string(),
                };
                let Some(tree) = mob.dialogue_tree else {
                    return "(no dialogue tree)".to_string();
                };
                let Some(node) = tree.nodes.get(&node_name) else {
                    return format!("(no node `{}`)", node_name);
                };
                let mut out = format!("Node `{}`", node_name);
                if tree.root_node == node_name {
                    out.push_str(" [root]");
                }
                out.push('\n');
                out.push_str(&format!("  text: {}\n", node.text));
                if node.choices.is_empty() {
                    out.push_str("  (no choices)\n");
                } else {
                    out.push_str("  choices:\n");
                    for (i, c) in node.choices.iter().enumerate() {
                        let target_tag = match &c.target {
                            DialogueTarget::Goto { node } => format!("-> {}", node),
                            DialogueTarget::Exit => "[exit]".to_string(),
                            DialogueTarget::Repeat => "[repeat]".to_string(),
                        };
                        let mut slice3_tags = String::new();
                        if let Some(cd) = c.cooldown_secs.filter(|n| *n > 0) {
                            slice3_tags.push_str(&format!(" [cd:{}s]", cd));
                        }
                        if c.once_per_player {
                            slice3_tags.push_str(" [once]");
                        }
                        out.push_str(&format!(
                            "    {}. [{}] {} {}{}\n",
                            i, c.keyword, c.label, target_tag, slice3_tags
                        ));
                        for (ci, cond) in c.conditions.iter().enumerate() {
                            out.push_str(&format!(
                                "         if [{}] {}\n",
                                ci,
                                summarize_condition(cond)
                            ));
                        }
                        for (ei, eff) in c.effects.iter().enumerate() {
                            out.push_str(&format!(
                                "         do [{}] {}\n",
                                ei,
                                summarize_effect(eff)
                            ));
                        }
                        if let Some(hint) = c.hint.as_ref().filter(|s| !s.is_empty()) {
                            out.push_str(&format!("         hint: {}\n", hint));
                        }
                    }
                }
                let counts = (
                    node.on_enter.len(),
                    node.on_each_visit.len(),
                    node.on_exit.len(),
                );
                if counts.0 + counts.1 + counts.2 > 0 {
                    out.push_str(&format!(
                        "  triggers: on_enter={}, on_each_visit={}, on_exit={}\n",
                        counts.0, counts.1, counts.2
                    ));
                }
                out
            },
        );
    }
}

fn parse_target(kind: &str, node: &str) -> Result<DialogueTarget, String> {
    match kind.to_lowercase().as_str() {
        "goto" => {
            if node.is_empty() {
                Err("goto target requires node name".into())
            } else {
                Ok(DialogueTarget::Goto {
                    node: node.to_string(),
                })
            }
        }
        "exit" => Ok(DialogueTarget::Exit),
        "repeat" => Ok(DialogueTarget::Repeat),
        other => Err(format!("unknown target kind `{}`", other)),
    }
}

fn summarize_condition(c: &DialogueCondition) -> String {
    match c {
        DialogueCondition::FlagSet { name, scope } => {
            format!("flag_set {} ({:?})", name, scope).to_lowercase()
        }
        DialogueCondition::FlagUnset { name, scope } => {
            format!("flag_unset {} ({:?})", name, scope).to_lowercase()
        }
        DialogueCondition::HasItem { vnum, qty } => format!("has_item {} x{}", vnum, qty),
        DialogueCondition::SkillAtLeast { key, level } => {
            format!("skill_at_least {} {}", key, level)
        }
        DialogueCondition::CounterAtLeast { key, value } => {
            format!("counter_at_least {} {}", key, value)
        }
        DialogueCondition::DgVarEquals { scope, key, value } => {
            format!("dg_var_equals {:?}:{} == {}", scope, key, value).to_lowercase()
        }
        DialogueCondition::QuestActive { vnum } => format!("quest_active {}", vnum),
        DialogueCondition::QuestComplete { vnum } => format!("quest_complete {}", vnum),
        DialogueCondition::QuestCompletable { vnum } => format!("quest_completable {}", vnum),
        DialogueCondition::HumanityAtLeast { threshold } => {
            format!("humanity_at_least {}", threshold)
        }
        DialogueCondition::IsThinblood => "is_thinblood".into(),
        DialogueCondition::IsClanAcknowledged => "is_clan_acknowledged".into(),
        DialogueCondition::HasAchievement { key } => format!("has_achievement {}", key),
        DialogueCondition::QuestChoiceEquals {
            quest_vnum,
            key,
            value,
        } => format!("quest_choice_equals {}:{} == {}", quest_vnum, key, value),
    }
}

fn summarize_effect(e: &DialogueEffect) -> String {
    match e {
        DialogueEffect::SetFlag { name, scope } => {
            format!("set_flag {} ({:?})", name, scope).to_lowercase()
        }
        DialogueEffect::ClearFlag { name, scope } => {
            format!("clear_flag {} ({:?})", name, scope).to_lowercase()
        }
        DialogueEffect::GiveItem { vnum, qty } => format!("give_item {} x{}", vnum, qty),
        DialogueEffect::TakeItem { vnum, qty } => format!("take_item {} x{}", vnum, qty),
        DialogueEffect::AwardSkillXp { skill, amount } => {
            format!("award_skill_xp {} {}", skill, amount)
        }
        DialogueEffect::SetCounter { key, value } => format!("set_counter {} {}", key, value),
        DialogueEffect::IncrementCounter { key, by } => {
            format!("increment_counter {} +{}", key, by)
        }
        DialogueEffect::SetDgVar { scope, key, value } => {
            format!("set_dg_var {:?}:{} = {}", scope, key, value).to_lowercase()
        }
        DialogueEffect::FireDgTrigger { trigger_type, arg } => {
            format!("fire_dg_trigger {} `{}`", trigger_type, arg)
        }
        DialogueEffect::OfferQuest { vnum } => format!("offer_quest {}", vnum),
        DialogueEffect::CompleteQuest { vnum } => format!("complete_quest {}", vnum),
        DialogueEffect::AbandonQuest { vnum } => format!("abandon_quest {}", vnum),
        DialogueEffect::SetQuestChoice {
            quest_vnum,
            key,
            value,
        } => format!("set_quest_choice {}:{} = {}", quest_vnum, key, value),
    }
}

/// Parse a flag scope token. Defaults to `Local` when omitted/blank.
fn parse_flag_scope(token: Option<&str>) -> Result<FlagScope, String> {
    match token.map(|s| s.to_lowercase()).as_deref() {
        None | Some("") | Some("local") => Ok(FlagScope::Local),
        Some("global") => Ok(FlagScope::Global),
        Some(other) => Err(format!("unknown scope `{}` (use local|global)", other)),
    }
}

/// Parse a `DialogueCondition` from a `kind` keyword and a space-separated
/// `args` string. Only the common condition kinds are inlinable; rarer
/// variants (DgVarEquals, IsThinblood, IsClanAcknowledged, QuestChoiceEquals,
/// HumanityAtLeast) stay JSON/MCP-only.
fn parse_choice_condition(kind: &str, args: &str) -> Result<DialogueCondition, String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let need = |n: usize, fmt: &str| -> Result<(), String> {
        if parts.len() < n {
            Err(format!("{} requires {}", kind, fmt))
        } else {
            Ok(())
        }
    };
    match kind.to_lowercase().as_str() {
        "flag_set" | "flagset" => {
            need(1, "<name> [local|global]")?;
            Ok(DialogueCondition::FlagSet {
                name: parts[0].to_string(),
                scope: parse_flag_scope(parts.get(1).copied())?,
            })
        }
        "flag_unset" | "flagunset" => {
            need(1, "<name> [local|global]")?;
            Ok(DialogueCondition::FlagUnset {
                name: parts[0].to_string(),
                scope: parse_flag_scope(parts.get(1).copied())?,
            })
        }
        "has_item" | "hasitem" => {
            need(1, "<vnum> [qty]")?;
            let qty = parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            Ok(DialogueCondition::HasItem {
                vnum: parts[0].to_string(),
                qty,
            })
        }
        "skill_at_least" | "skillatleast" | "skill" => {
            need(2, "<skill_key> <level>")?;
            let level = parts[1]
                .parse::<i32>()
                .map_err(|_| "level must be an integer".to_string())?;
            Ok(DialogueCondition::SkillAtLeast {
                key: parts[0].to_string(),
                level,
            })
        }
        "counter_at_least" | "counteratleast" | "counter" => {
            need(2, "<counter_key> <value>")?;
            let value = parts[1]
                .parse::<i32>()
                .map_err(|_| "value must be an integer".to_string())?;
            Ok(DialogueCondition::CounterAtLeast {
                key: parts[0].to_string(),
                value,
            })
        }
        "quest_active" | "questactive" => {
            need(1, "<quest_vnum>")?;
            Ok(DialogueCondition::QuestActive {
                vnum: parts[0].to_string(),
            })
        }
        "quest_complete" | "questcomplete" => {
            need(1, "<quest_vnum>")?;
            Ok(DialogueCondition::QuestComplete {
                vnum: parts[0].to_string(),
            })
        }
        "quest_completable" | "questcompletable" => {
            need(1, "<quest_vnum>")?;
            Ok(DialogueCondition::QuestCompletable {
                vnum: parts[0].to_string(),
            })
        }
        "has_achievement" | "hasachievement" | "achievement" => {
            need(1, "<achievement_key>")?;
            Ok(DialogueCondition::HasAchievement {
                key: parts[0].to_string(),
            })
        }
        other => Err(format!(
            "unknown condition kind `{}` (use flag_set|flag_unset|has_item|skill_at_least|counter_at_least|quest_active|quest_complete|quest_completable|has_achievement; use `tree set <json>` or MCP for dg_var_equals / vampire / quest_choice_equals)",
            other
        )),
    }
}

/// Parse a `DialogueEffect` from a `kind` keyword and a space-separated
/// `args` string. Rarer variants (SetDgVar, FireDgTrigger, SetQuestChoice)
/// stay JSON/MCP-only.
fn parse_choice_effect(kind: &str, args: &str) -> Result<DialogueEffect, String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let need = |n: usize, fmt: &str| -> Result<(), String> {
        if parts.len() < n {
            Err(format!("{} requires {}", kind, fmt))
        } else {
            Ok(())
        }
    };
    match kind.to_lowercase().as_str() {
        "set_flag" | "setflag" => {
            need(1, "<name> [local|global]")?;
            Ok(DialogueEffect::SetFlag {
                name: parts[0].to_string(),
                scope: parse_flag_scope(parts.get(1).copied())?,
            })
        }
        "clear_flag" | "clearflag" => {
            need(1, "<name> [local|global]")?;
            Ok(DialogueEffect::ClearFlag {
                name: parts[0].to_string(),
                scope: parse_flag_scope(parts.get(1).copied())?,
            })
        }
        "give_item" | "giveitem" => {
            need(1, "<vnum> [qty]")?;
            let qty = parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            Ok(DialogueEffect::GiveItem {
                vnum: parts[0].to_string(),
                qty,
            })
        }
        "take_item" | "takeitem" => {
            need(1, "<vnum> [qty]")?;
            let qty = parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            Ok(DialogueEffect::TakeItem {
                vnum: parts[0].to_string(),
                qty,
            })
        }
        "award_skill_xp" | "awardskillxp" | "award_xp" => {
            need(2, "<skill_key> <amount>")?;
            let amount = parts[1]
                .parse::<i32>()
                .map_err(|_| "amount must be an integer".to_string())?;
            Ok(DialogueEffect::AwardSkillXp {
                skill: parts[0].to_string(),
                amount,
            })
        }
        "set_counter" | "setcounter" => {
            need(2, "<counter_key> <value>")?;
            let value = parts[1]
                .parse::<i32>()
                .map_err(|_| "value must be an integer".to_string())?;
            Ok(DialogueEffect::SetCounter {
                key: parts[0].to_string(),
                value,
            })
        }
        "increment_counter" | "incrementcounter" | "inc_counter" => {
            need(1, "<counter_key> [by]")?;
            let by = parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            Ok(DialogueEffect::IncrementCounter {
                key: parts[0].to_string(),
                by,
            })
        }
        "offer_quest" | "offerquest" => {
            need(1, "<quest_vnum>")?;
            Ok(DialogueEffect::OfferQuest {
                vnum: parts[0].to_string(),
            })
        }
        "complete_quest" | "completequest" => {
            need(1, "<quest_vnum>")?;
            Ok(DialogueEffect::CompleteQuest {
                vnum: parts[0].to_string(),
            })
        }
        "abandon_quest" | "abandonquest" => {
            need(1, "<quest_vnum>")?;
            Ok(DialogueEffect::AbandonQuest {
                vnum: parts[0].to_string(),
            })
        }
        other => Err(format!(
            "unknown effect kind `{}` (use set_flag|clear_flag|give_item|take_item|award_skill_xp|set_counter|increment_counter|offer_quest|complete_quest|abandon_quest; use `tree set <json>` or MCP for set_dg_var / fire_dg_trigger / set_quest_choice)",
            other
        )),
    }
}

fn mutate_tree<F>(db: &Db, mobile_id: &str, op: F) -> String
where
    F: FnOnce(&mut Option<DialogueTree>) -> Result<(), crate::dialogue_edit::DialogueEditError>,
{
    let mob_uuid = match Uuid::parse_str(mobile_id) {
        Ok(u) => u,
        Err(_) => return "invalid mobile id".to_string(),
    };
    let mut mob = match db.get_mobile_data(&mob_uuid).ok().flatten() {
        Some(m) => m,
        None => return "mobile not found".to_string(),
    };
    if let Err(e) = op(&mut mob.dialogue_tree) {
        return e.to_string();
    }
    if let Err(e) = db.save_mobile_data(mob) {
        return format!("save error: {}", e);
    }
    String::new()
}

// ===== Public Rust-side helpers (for src/lib.rs input dispatcher) =====

/// True iff the player at this connection is in sticky dialogue mode.
pub fn session_in_dialogue(connections: &SharedConnections, connection_id: &Uuid) -> bool {
    let conns = connections.lock().unwrap();
    conns
        .get(connection_id)
        .map(|s| s.dialogue_partner_id.is_some())
        .unwrap_or(false)
}

/// A mob's spoken response that the caller (lib.rs) is expected to garble
/// per-listener and emit. Carries the raw response + language key so the
/// caller can apply `garble_for_listener` based on each listener's skill.
#[derive(Debug, Clone)]
pub struct DialogueSayLine {
    pub mob_short: String,
    /// Ungarbled response text. lib.rs prepends `"<short> says: "`.
    pub raw_response: String,
    /// Mob's spoken_language. Empty / lingua-franca short-circuits to plain.
    pub language_key: String,
    pub room_id: Uuid,
}

/// Outcome of a sticky-mode input dispatch.
#[derive(Debug, Clone)]
pub enum DialogueDispatch {
    /// Input handled inside dialogue. Output already collected.
    Handled {
        /// Lines to send to the actor only (e.g. menu).
        actor_lines: Vec<String>,
        /// Room broadcasts: (room_id, message). Sent to all but actor.
        room_broadcasts: Vec<(Uuid, String)>,
        /// Mob speech to emit per-listener (with language-aware garbling).
        speech: Option<DialogueSayLine>,
    },
    /// Player ended dialogue and the input should fall through to normal
    /// command parsing (e.g. typed a movement direction).
    ExitedFallthrough {
        actor_lines: Vec<String>,
        room_broadcasts: Vec<(Uuid, String)>,
        speech: Option<DialogueSayLine>,
    },
    /// Not in dialogue, or dialogue couldn't handle this input. Caller
    /// should run normal command parsing as if dialogue weren't active.
    Fallthrough,
}

/// Drive sticky-mode input. Caller (`src/lib.rs` input loop) checks
/// `session_in_dialogue` first; if true, calls this. The dispatch:
///   - "1".."9"               -> walk_dialogue_choice
///   - "bye" / "leave" / "quit" -> exit_dialogue, Handled
///   - direction              -> exit_dialogue, ExitedFallthrough
///   - any other word         -> walk_dialogue_keyword; Handled on hit,
///                               otherwise Fallthrough (no exit)
pub fn dispatch_sticky_input(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    connection_id: Uuid,
    input: &str,
) -> DialogueDispatch {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DialogueDispatch::Fallthrough;
    }
    let first_word = trimmed.split_whitespace().next().unwrap_or("").to_lowercase();

    // Numeric choice (1-9, single digit only — multi-digit menus are deferred).
    if first_word.len() == 1 {
        if let Some(c) = first_word.chars().next() {
            if let Some(idx) = c.to_digit(10) {
                if idx >= 1 {
                    return walk_choice_internal(db, connections, state, connection_id, idx as i64);
                }
            }
        }
    }

    // Exit phrases.
    if matches!(first_word.as_str(), "bye" | "leave" | "quit") {
        return exit_internal(db, connections, state, connection_id);
    }

    // Movement directions exit and fall through.
    if is_movement_direction(&first_word) {
        let outcome = exit_internal(db, connections, state, connection_id);
        return match outcome {
            DialogueDispatch::Handled {
                actor_lines,
                room_broadcasts,
                speech,
            } => DialogueDispatch::ExitedFallthrough {
                actor_lines,
                room_broadcasts,
                speech,
            },
            other => other,
        };
    }

    // Otherwise try to walk by keyword (whole-word match).
    walk_keyword_internal(db, connections, state, connection_id, &first_word)
}

fn walk_choice_internal(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    connection_id: Uuid,
    idx: i64,
) -> DialogueDispatch {
    let mut actor_lines = Vec::new();
    let room_broadcasts: Vec<(Uuid, String)> = Vec::new();

    let Some(partner_id) = connection_partner(connections, connection_id) else {
        return DialogueDispatch::Fallthrough;
    };
    let Some(mob) = db.get_mobile_data(&partner_id).ok().flatten() else {
        clear_partner(connections, connection_id);
        return DialogueDispatch::Fallthrough;
    };
    if mob.dialogue_tree.is_none() {
        clear_partner(connections, connection_id);
        return DialogueDispatch::Fallthrough;
    }
    let Some(mut ch) = get_character_for_conn(connections, connection_id) else {
        return DialogueDispatch::Fallthrough;
    };

    let cur = current_node_for(&ch, &mob);
    let tree = mob.dialogue_tree.as_ref().unwrap();
    let Some(node) = tree.nodes.get(&cur) else {
        return DialogueDispatch::Fallthrough;
    };
    let classified = classify_choices(&cur, node, &ch, &mob, db, now_epoch_secs());
    if idx < 1 || (idx as usize) > classified.len() {
        actor_lines.push(format!("Choose 1-{} or 'bye' to leave.", classified.len()));
        return DialogueDispatch::Handled {
            actor_lines,
            room_broadcasts,
            speech: None,
        };
    }
    let entry = classified[(idx as usize) - 1].clone();
    let (response, menu, finished) = match &entry.visibility {
        ChoiceVisibility::Available => {
            take_choice_at_node(db, connections, state, &mut ch, &mob, &cur, &entry.choice)
        }
        ChoiceVisibility::Locked { .. } => (
            "That doesn't seem available right now.".to_string(),
            render_classified_menu(&classified),
            false,
        ),
        ChoiceVisibility::Cooldown { .. } => (
            "You'll need to wait before raising that again.".to_string(),
            render_classified_menu(&classified),
            false,
        ),
    };
    save_character_and_sync(db, connections, connection_id, ch);
    if finished {
        clear_partner(connections, connection_id);
    }
    let mob_room = mob.current_room_id.unwrap_or(Uuid::nil());
    let speech = if response.is_empty() {
        None
    } else {
        Some(DialogueSayLine {
            mob_short: mob.short_desc.clone(),
            raw_response: response,
            language_key: mob.spoken_language.clone().unwrap_or_default(),
            room_id: mob_room,
        })
    };
    if !finished && !menu.is_empty() {
        actor_lines.push(menu);
    }
    if finished {
        actor_lines.push(format!("(You step back from {}.)", mob.short_desc));
    }
    DialogueDispatch::Handled {
        actor_lines,
        room_broadcasts,
        speech,
    }
}

fn walk_keyword_internal(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    connection_id: Uuid,
    keyword: &str,
) -> DialogueDispatch {
    let mut actor_lines = Vec::new();
    let room_broadcasts: Vec<(Uuid, String)> = Vec::new();

    let Some(partner_id) = connection_partner(connections, connection_id) else {
        return DialogueDispatch::Fallthrough;
    };
    let Some(mob) = db.get_mobile_data(&partner_id).ok().flatten() else {
        clear_partner(connections, connection_id);
        return DialogueDispatch::Fallthrough;
    };
    if mob.dialogue_tree.is_none() {
        clear_partner(connections, connection_id);
        return DialogueDispatch::Fallthrough;
    }
    let Some(mut ch) = get_character_for_conn(connections, connection_id) else {
        return DialogueDispatch::Fallthrough;
    };

    let cur = current_node_for(&ch, &mob);
    let tree = mob.dialogue_tree.as_ref().unwrap();
    let Some(node) = tree.nodes.get(&cur) else {
        return DialogueDispatch::Fallthrough;
    };
    let classified = classify_choices(&cur, node, &ch, &mob, db, now_epoch_secs());
    let Some(entry) = classified
        .iter()
        .find(|e| e.choice.keyword.eq_ignore_ascii_case(keyword))
    else {
        return DialogueDispatch::Fallthrough;
    };
    let entry = entry.clone();
    let (response, menu, finished) = match &entry.visibility {
        ChoiceVisibility::Available => {
            take_choice_at_node(db, connections, state, &mut ch, &mob, &cur, &entry.choice)
        }
        ChoiceVisibility::Locked { .. } => (
            "That doesn't seem available right now.".to_string(),
            render_classified_menu(&classified),
            false,
        ),
        ChoiceVisibility::Cooldown { .. } => (
            "You'll need to wait before raising that again.".to_string(),
            render_classified_menu(&classified),
            false,
        ),
    };
    save_character_and_sync(db, connections, connection_id, ch);
    if finished {
        clear_partner(connections, connection_id);
    }
    let mob_room = mob.current_room_id.unwrap_or(Uuid::nil());
    let speech = if response.is_empty() {
        None
    } else {
        Some(DialogueSayLine {
            mob_short: mob.short_desc.clone(),
            raw_response: response,
            language_key: mob.spoken_language.clone().unwrap_or_default(),
            room_id: mob_room,
        })
    };
    if !finished && !menu.is_empty() {
        actor_lines.push(menu);
    }
    if finished {
        actor_lines.push(format!("(You step back from {}.)", mob.short_desc));
    }
    DialogueDispatch::Handled {
        actor_lines,
        room_broadcasts,
        speech,
    }
}

fn exit_internal(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    connection_id: Uuid,
) -> DialogueDispatch {
    let mut actor_lines = Vec::new();
    let mut room_broadcasts = Vec::new();
    let Some(partner_id) = connection_partner(connections, connection_id) else {
        return DialogueDispatch::Fallthrough;
    };
    clear_partner(connections, connection_id);
    if let Some(mob) = db.get_mobile_data(&partner_id).ok().flatten() {
        let mut exit_msg = String::new();
        if let Some(mut ch) = get_character_for_conn(connections, connection_id) {
            let cur = current_node_for(&ch, &mob);
            exit_msg = exit_node(db, connections, state, &mut ch, &mob, &cur);
            if let Some(s) = ch.dialogue_pair_state.get_mut(&mob.vnum) {
                s.current_node = None;
            }
            save_character_and_sync(db, connections, connection_id, ch);
        }
        if !exit_msg.is_empty() {
            // on_exit effect-messages are bracketed status lines ("[ You
            // receive: ... ]"), not speech — actor-only, no room broadcast.
            actor_lines.push(exit_msg);
        }
        actor_lines.push(format!("(You step back from {}.)", mob.short_desc));
        let mob_room = mob.current_room_id.unwrap_or(Uuid::nil());
        room_broadcasts.push((mob_room, format!("{} turns away.", mob.short_desc)));
    }
    DialogueDispatch::Handled {
        actor_lines,
        room_broadcasts,
        speech: None,
    }
}

fn connection_partner(connections: &SharedConnections, conn_id: Uuid) -> Option<Uuid> {
    let conns = connections.lock().unwrap();
    conns.get(&conn_id).and_then(|s| s.dialogue_partner_id)
}

fn clear_partner(connections: &SharedConnections, conn_id: Uuid) {
    let mut conns = connections.lock().unwrap();
    if let Some(s) = conns.get_mut(&conn_id) {
        s.dialogue_partner_id = None;
    }
}

fn is_movement_direction(word: &str) -> bool {
    matches!(
        word,
        "n" | "north"
            | "s"
            | "south"
            | "e"
            | "east"
            | "w"
            | "west"
            | "u"
            | "up"
            | "d"
            | "down"
            | "ne"
            | "northeast"
            | "nw"
            | "northwest"
            | "se"
            | "southeast"
            | "sw"
            | "southwest"
            | "go"
    )
}

// ===== Internal helpers =====

fn empty_result() -> Map {
    let mut m = Map::new();
    m.insert("ok".into(), rhai::Dynamic::from(false));
    m.insert("error".into(), rhai::Dynamic::from(String::new()));
    m.insert("mob_id".into(), rhai::Dynamic::from(String::new()));
    m.insert("mob_short_desc".into(), rhai::Dynamic::from(String::new()));
    m.insert("mob_room_id".into(), rhai::Dynamic::from(String::new()));
    m.insert("response".into(), rhai::Dynamic::from(String::new()));
    m.insert("menu".into(), rhai::Dynamic::from(String::new()));
    m.insert("finished".into(), rhai::Dynamic::from(false));
    m
}

fn err(mut m: Map, msg: &str) -> Map {
    m.insert("ok".into(), rhai::Dynamic::from(false));
    m.insert("error".into(), rhai::Dynamic::from(msg.to_string()));
    m
}

fn session_dialogue_partner(connections: &SharedConnections, conn_id_str: &str) -> Option<Uuid> {
    let uuid = Uuid::parse_str(conn_id_str).ok()?;
    let conns = connections.lock().unwrap();
    conns.get(&uuid).and_then(|s| s.dialogue_partner_id)
}

fn set_session_dialogue_partner(
    connections: &SharedConnections,
    conn_id: Uuid,
    partner: Option<Uuid>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(s) = conns.get_mut(&conn_id) {
        s.dialogue_partner_id = partner;
    }
}

fn get_character_for_conn(connections: &SharedConnections, conn_id: Uuid) -> Option<CharacterData> {
    let conns = connections.lock().unwrap();
    conns.get(&conn_id).and_then(|s| s.character.clone())
}

/// Persist `ch` to the DB AND write it back to `session.character`.
/// Dialogue walks mutate `dialogue_pair_state.current_node` on a clone of the
/// session character; without syncing the session, the next input clones the
/// stale pre-walk view and the player gets stuck on the root node.
fn save_character_and_sync(
    db: &Db,
    connections: &SharedConnections,
    conn_id: Uuid,
    ch: CharacterData,
) {
    let _ = db.save_character_data(ch.clone());
    if let Ok(mut conns) = connections.lock() {
        if let Some(session) = conns.get_mut(&conn_id) {
            session.character = Some(ch);
        }
    }
}

fn find_mob_in_room_with_tree(db: &Db, room_id: Uuid, keyword: &str) -> Option<MobileData> {
    let needle = keyword.to_lowercase();
    let mobs = db.get_mobiles_in_room(&room_id).ok()?;
    for m in mobs {
        if !m.is_prototype && m.dialogue_tree.is_some() {
            if mob_matches_keyword(&m, &needle) {
                return Some(m);
            }
        }
    }
    None
}

fn list_room_mobs_with_trees(db: &Db, room_id: Uuid) -> Vec<MobileData> {
    db.get_mobiles_in_room(&room_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| !m.is_prototype && m.dialogue_tree.is_some())
        .collect()
}

fn mob_matches_keyword(mob: &MobileData, needle: &str) -> bool {
    if mob.name.to_lowercase().contains(needle) {
        return true;
    }
    for k in &mob.keywords {
        if k.to_lowercase().starts_with(needle) {
            return true;
        }
    }
    false
}

fn current_node_for(ch: &CharacterData, mob: &MobileData) -> String {
    let tree = mob.dialogue_tree.as_ref().expect("tree present");
    if let Some(state) = ch.dialogue_pair_state.get(&mob.vnum) {
        if let Some(n) = state.current_node.as_deref() {
            if tree.nodes.contains_key(n) {
                return n.to_string();
            }
        }
    }
    tree.root_node.clone()
}

struct ViewParts {
    response: String,
    menu: String,
    finished: bool,
}

fn enter_root_or_keep(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
) -> ViewParts {
    // If the player already had a current_node from a prior session, treat
    // this as a resume — do NOT fire entry triggers, just re-render the menu.
    let had_cursor = ch
        .dialogue_pair_state
        .get(&mob.vnum)
        .and_then(|s| s.current_node.as_ref())
        .map(|n| {
            mob.dialogue_tree
                .as_ref()
                .map(|t| t.nodes.contains_key(n))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let cur = current_node_for(ch, mob);
    let tree = mob.dialogue_tree.as_ref().unwrap();
    if !tree.nodes.contains_key(&cur) {
        return ViewParts {
            response: format!("(no node `{}`)", cur),
            menu: String::new(),
            finished: true,
        };
    }
    let extra = if had_cursor {
        // Resume: bump last_seen but skip on_enter / on_each_visit.
        set_current_node(ch, &mob.vnum, Some(&cur));
        String::new()
    } else {
        // Fresh start: enter the node properly.
        enter_node(db, connections, state, ch, mob, &cur)
    };
    let tree = mob.dialogue_tree.as_ref().unwrap();
    let node = tree.nodes.get(&cur).unwrap();
    let menu = render_classified_menu(&classify_choices(
        &cur,
        node,
        ch,
        mob,
        db,
        now_epoch_secs(),
    ));
    let response = if extra.is_empty() {
        node.text.clone()
    } else {
        format!("{}\n{}", node.text, extra)
    };
    ViewParts {
        response,
        menu,
        finished: false,
    }
}

/// Move the cursor to `node_name`, firing on_each_visit (always) and on_enter
/// (first visit only). Bumps the per-node visit counter. Returns the
/// concatenated effect-message lines.
fn enter_node(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
    node_name: &str,
) -> String {
    set_current_node(ch, &mob.vnum, Some(node_name));
    let tree = match mob.dialogue_tree.as_ref() {
        Some(t) => t,
        None => return String::new(),
    };
    let node = match tree.nodes.get(node_name) {
        Some(n) => n.clone(),
        None => return String::new(),
    };
    let prior_visits = ch
        .dialogue_pair_state
        .get(&mob.vnum)
        .and_then(|s| s.visit_counts.get(node_name))
        .copied()
        .unwrap_or(0);
    // Increment visit counter BEFORE running effects so on_enter sees the new
    // count (in case effects read counters via Rust APIs in the future).
    bump_visit_count(ch, &mob.vnum, node_name);
    let mut messages = String::new();
    if !node.on_each_visit.is_empty() {
        let m = apply_effects_collect_messages(db, connections, state, ch, mob, &node.on_each_visit);
        if !m.is_empty() {
            messages.push_str(&m);
        }
    }
    if prior_visits == 0 && !node.on_enter.is_empty() {
        let m = apply_effects_collect_messages(db, connections, state, ch, mob, &node.on_enter);
        if !m.is_empty() {
            if !messages.is_empty() {
                messages.push('\n');
            }
            messages.push_str(&m);
        }
    }
    messages
}

/// Fire the current node's on_exit effects (if the named node exists).
/// Caller is responsible for then setting the cursor to the new node (or None).
fn exit_node(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
    node_name: &str,
) -> String {
    let tree = match mob.dialogue_tree.as_ref() {
        Some(t) => t,
        None => return String::new(),
    };
    let node = match tree.nodes.get(node_name) {
        Some(n) => n.clone(),
        None => return String::new(),
    };
    if node.on_exit.is_empty() {
        return String::new();
    }
    apply_effects_collect_messages(db, connections, state, ch, mob, &node.on_exit)
}

fn bump_visit_count(ch: &mut CharacterData, vnum: &str, node_name: &str) {
    let entry = ch
        .dialogue_pair_state
        .entry(vnum.to_string())
        .or_insert_with(DialoguePairState::default);
    let counter = entry.visit_counts.entry(node_name.to_string()).or_insert(0);
    *counter = counter.saturating_add(1);
}

#[cfg(test)]
fn take_choice(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
    choice: &DialogueChoice,
) -> (String, String, bool) {
    let cur = current_node_for(ch, mob);
    take_choice_at_node(db, connections, state, ch, mob, &cur, choice)
}

fn take_choice_at_node(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
    src_node_name: &str,
    choice: &DialogueChoice,
) -> (String, String, bool) {
    // 0. Record cooldown / once-per-player BEFORE effects execute, so an
    //    effect that exits dialogue still leaves the marker behind.
    record_choice_pick(ch, &mob.vnum, src_node_name, choice);
    // 1. Apply the choice's effects.
    let mut effect_messages = apply_effects_collect_messages(db, connections, state, ch, mob, &choice.effects);
    // 2. Navigate.
    match &choice.target {
        DialogueTarget::Exit => {
            let exit_msg = exit_node(db, connections, state, ch, mob, src_node_name);
            append_msg(&mut effect_messages, &exit_msg);
            set_current_node(ch, &mob.vnum, None);
            let response = if effect_messages.is_empty() {
                String::new()
            } else {
                effect_messages
            };
            (response, String::new(), true)
        }
        DialogueTarget::Goto { node } => {
            // Validate node exists.
            let tree = mob.dialogue_tree.as_ref().unwrap();
            if !tree.nodes.contains_key(node) {
                set_current_node(ch, &mob.vnum, None);
                return (
                    format!("(broken target: node `{}`)", node),
                    String::new(),
                    true,
                );
            }
            // Fire on_exit for the source node before moving.
            let exit_msg = exit_node(db, connections, state, ch, mob, src_node_name);
            append_msg(&mut effect_messages, &exit_msg);
            // Enter the target — fires on_each_visit + on_enter (first visit).
            let entry_msg = enter_node(db, connections, state, ch, mob, node);
            append_msg(&mut effect_messages, &entry_msg);
            let tree = mob.dialogue_tree.as_ref().unwrap();
            let target = tree.nodes.get(node).unwrap();
            let menu = render_classified_menu(&classify_choices(
                node,
                target,
                ch,
                mob,
                db,
                now_epoch_secs(),
            ));
            let response = if effect_messages.is_empty() {
                target.text.clone()
            } else {
                format!("{}\n{}", target.text, effect_messages)
            };
            (response, menu, false)
        }
        DialogueTarget::Repeat => {
            // Repeat is a refresh — no exit/enter triggers fire.
            let tree = mob.dialogue_tree.as_ref().unwrap();
            let node = tree.nodes.get(src_node_name).unwrap();
            let menu = render_classified_menu(&classify_choices(
                src_node_name,
                node,
                ch,
                mob,
                db,
                now_epoch_secs(),
            ));
            let response = if effect_messages.is_empty() {
                node.text.clone()
            } else {
                format!("{}\n{}", node.text, effect_messages)
            };
            (response, menu, false)
        }
    }
}

/// Record cooldown timestamp and once-per-player marker on a successful pick.
fn record_choice_pick(
    ch: &mut CharacterData,
    vnum: &str,
    node_name: &str,
    choice: &DialogueChoice,
) {
    if choice.cooldown_secs.unwrap_or(0) <= 0 && !choice.once_per_player {
        return;
    }
    let entry = ch
        .dialogue_pair_state
        .entry(vnum.to_string())
        .or_insert_with(DialoguePairState::default);
    let key = cooldown_key(node_name, &choice.keyword);
    if choice.cooldown_secs.filter(|n| *n > 0).is_some() {
        entry.choice_cooldowns.insert(key.clone(), now_epoch_secs());
    }
    if choice.once_per_player {
        entry.choices_picked_once.insert(key);
    }
}

fn append_msg(target: &mut String, addition: &str) {
    if addition.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

fn set_current_node(ch: &mut CharacterData, vnum: &str, node: Option<&str>) {
    let entry = ch
        .dialogue_pair_state
        .entry(vnum.to_string())
        .or_insert_with(DialoguePairState::default);
    entry.current_node = node.map(|s| s.to_string());
    entry.last_seen_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
}

/// Classification for a single choice that survived the silent-hide filter.
/// Hidden choices (failed conditions with no hint, or once_per_player already
/// picked) are dropped from `classify_choices` output entirely.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ChoiceVisibility {
    Available,
    Locked { hint: String },
    Cooldown { remaining_secs: i64 },
}

#[derive(Clone, Debug)]
pub(crate) struct ClassifiedChoice {
    pub choice: DialogueChoice,
    pub visibility: ChoiceVisibility,
}

fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cooldown_key(node_name: &str, keyword: &str) -> String {
    format!("{}:{}", node_name, keyword)
}

/// Classify each choice on `node` for the given (player, mob) pair. Hidden
/// entries (silently-failed conditions, or `once_per_player` already picked)
/// are dropped from the returned list. Locked-with-hint and Cooldown entries
/// are kept so the menu renderer can show flavor lines for them.
fn classify_choices(
    node_name: &str,
    node: &DialogueNode,
    ch: &CharacterData,
    mob: &MobileData,
    db: &Db,
    now_secs: i64,
) -> Vec<ClassifiedChoice> {
    let pair = ch.dialogue_pair_state.get(&mob.vnum);
    let mut out = Vec::with_capacity(node.choices.len());
    for c in node.choices.iter() {
        let key = cooldown_key(node_name, &c.keyword);
        // once_per_player wins outright when already picked.
        if c.once_per_player
            && pair
                .map(|s| s.choices_picked_once.contains(&key))
                .unwrap_or(false)
        {
            continue;
        }
        let conditions_ok = c
            .conditions
            .iter()
            .all(|cond| evaluate_condition(cond, ch, mob, db));
        if !conditions_ok {
            if let Some(hint) = c.hint.as_ref().filter(|s| !s.is_empty()) {
                out.push(ClassifiedChoice {
                    choice: c.clone(),
                    visibility: ChoiceVisibility::Locked { hint: hint.clone() },
                });
            }
            // No hint => silently hidden.
            continue;
        }
        // Conditions pass. Check cooldown.
        if let Some(cd) = c.cooldown_secs.filter(|n| *n > 0) {
            let last = pair
                .and_then(|s| s.choice_cooldowns.get(&key).copied())
                .unwrap_or(0);
            let elapsed = now_secs.saturating_sub(last);
            if elapsed < cd {
                let remaining = cd - elapsed;
                out.push(ClassifiedChoice {
                    choice: c.clone(),
                    visibility: ChoiceVisibility::Cooldown {
                        remaining_secs: remaining,
                    },
                });
                continue;
            }
        }
        out.push(ClassifiedChoice {
            choice: c.clone(),
            visibility: ChoiceVisibility::Available,
        });
    }
    out
}

fn fmt_cooldown(secs: i64) -> String {
    if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m{}s", m, s)
        }
    } else {
        format!("{}s", secs.max(1))
    }
}

fn render_classified_menu(entries: &[ClassifiedChoice]) -> String {
    if entries.is_empty() {
        return "  bye. (leave)".to_string();
    }
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        match &e.visibility {
            ChoiceVisibility::Available => {
                out.push_str(&format!("  {}. {}\n", i + 1, e.choice.label));
            }
            ChoiceVisibility::Locked { hint } => {
                out.push_str(&format!("  {}. (?) {}\n", i + 1, hint));
            }
            ChoiceVisibility::Cooldown { remaining_secs } => {
                out.push_str(&format!(
                    "  {}. (available in {}) {}\n",
                    i + 1,
                    fmt_cooldown(*remaining_secs),
                    e.choice.label
                ));
            }
        }
    }
    out.push_str("  bye. (leave)");
    out
}

/// Back-compat wrapper for the in-file test that only needs the pickable
/// subset of choices.
#[cfg(test)]
fn visible_choices<'a>(
    node: &'a DialogueNode,
    ch: &CharacterData,
    mob: &MobileData,
    db: &Db,
) -> Vec<DialogueChoice> {
    let cur = current_node_for(ch, mob);
    classify_choices(&cur, node, ch, mob, db, now_epoch_secs())
        .into_iter()
        .filter(|e| matches!(e.visibility, ChoiceVisibility::Available))
        .map(|e| e.choice)
        .collect()
}

fn evaluate_condition(cond: &DialogueCondition, ch: &CharacterData, mob: &MobileData, db: &Db) -> bool {
    match cond {
        DialogueCondition::FlagSet { name, scope } => is_flag_set(ch, name, *scope, &mob.vnum),
        DialogueCondition::FlagUnset { name, scope } => !is_flag_set(ch, name, *scope, &mob.vnum),
        DialogueCondition::HasItem { vnum, qty } => count_inventory_vnum(db, &ch.name, vnum) >= *qty,
        DialogueCondition::SkillAtLeast { key, level } => ch
            .skills
            .get(&key.to_lowercase())
            .map(|p| p.level >= *level)
            .unwrap_or(false),
        DialogueCondition::CounterAtLeast { key, value } => ch
            .achievement_counters
            .get(key)
            .map(|v| *v as i32 >= *value)
            .unwrap_or(false),
        DialogueCondition::DgVarEquals { scope, key, value } => match scope {
            DgScope::Player => ch.dg_vars.get(key).map(|v| v == value).unwrap_or(false),
            DgScope::Mob => mob.dg_vars.get(key).map(|v| v == value).unwrap_or(false),
        },
        DialogueCondition::QuestActive { vnum } => ch.active_quests.contains_key(vnum),
        DialogueCondition::QuestComplete { vnum } => ch.completed_quests.contains(vnum),
        DialogueCondition::QuestCompletable { vnum } => {
            if !ch.active_quests.contains_key(vnum) {
                return false;
            }
            match db.get_quest_data(vnum) {
                Ok(Some(quest)) => crate::quest::is_completable(db, ch, &quest),
                _ => false,
            }
        }
        DialogueCondition::HumanityAtLeast { threshold } => ch
            .vampire_state
            .as_ref()
            .map(|v| v.humanity >= *threshold)
            .unwrap_or(false),
        DialogueCondition::IsThinblood => crate::script::vampire::is_pc_thinblood(ch),
        DialogueCondition::IsClanAcknowledged => {
            ch.vampire_state.is_some()
                && crate::script::vampire::pc_clan_from_traits(ch).is_some()
        }
        DialogueCondition::HasAchievement { key } => ch.achievements_unlocked.contains_key(key),
        DialogueCondition::QuestChoiceEquals {
            quest_vnum,
            key,
            value,
        } => ch
            .active_quests
            .get(quest_vnum)
            .and_then(|aq| aq.choice_vars.get(key))
            .map(|v| v == value)
            .unwrap_or(false),
    }
}

fn apply_effects_collect_messages(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
    effects: &[DialogueEffect],
) -> String {
    let mut messages: Vec<String> = Vec::new();
    for e in effects {
        if let Some(msg) = apply_effect(db, connections, state, ch, mob, e) {
            messages.push(msg);
        }
    }
    messages.join("\n")
}

fn apply_effect(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    ch: &mut CharacterData,
    mob: &MobileData,
    effect: &DialogueEffect,
) -> Option<String> {
    match effect {
        DialogueEffect::SetFlag { name, scope } => {
            ch.dialogue_flags
                .insert(flag_key(name, *scope, &mob.vnum), true);
            None
        }
        DialogueEffect::ClearFlag { name, scope } => {
            ch.dialogue_flags.remove(&flag_key(name, *scope, &mob.vnum));
            None
        }
        DialogueEffect::GiveItem { vnum, qty } => {
            let mut given = 0;
            for _ in 0..*qty {
                match db.spawn_item_from_prototype(vnum) {
                    Ok(Some(mut item)) => {
                        item.location = crate::types::ItemLocation::Inventory(ch.name.clone());
                        if db.save_item_data(item.clone()).is_ok() {
                            given += 1;
                        }
                    }
                    _ => break,
                }
            }
            if given > 0 {
                let label = db
                    .get_item_by_vnum(vnum)
                    .ok()
                    .flatten()
                    .map(|i| i.short_desc)
                    .unwrap_or_else(|| format!("item {}", vnum));
                if given == *qty {
                    Some(format!("[ You receive: {} ]", label))
                } else {
                    Some(format!(
                        "[ You receive: {} (could only spawn {}/{}) ]",
                        label, given, qty
                    ))
                }
            } else {
                Some(format!("[ Could not deliver item {} ]", vnum))
            }
        }
        DialogueEffect::TakeItem { vnum, qty } => {
            let mut taken = 0;
            for _ in 0..*qty {
                if let Some(item_id) = find_inventory_item_uuid_by_vnum(db, &ch.name, vnum) {
                    if db.delete_item(&item_id).is_ok() {
                        taken += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            if taken == *qty {
                let label = db
                    .get_item_by_vnum(vnum)
                    .ok()
                    .flatten()
                    .map(|i| i.short_desc)
                    .unwrap_or_else(|| format!("item {}", vnum));
                Some(format!("[ You hand over: {} ]", label))
            } else {
                Some(format!(
                    "[ Tried to take {} {}; only {} taken ]",
                    qty, vnum, taken
                ))
            }
        }
        DialogueEffect::AwardSkillXp { skill, amount } => {
            let key = skill.to_lowercase();
            let entry = ch
                .skills
                .entry(key.clone())
                .or_insert(crate::SkillProgress::default());
            if entry.level >= 10 {
                return None;
            }
            entry.experience += *amount;
            // Threshold = 100 per level (matches existing add_skill_experience pattern minus traits).
            let mut leveled = false;
            while entry.experience >= 100 && entry.level < 10 {
                entry.experience -= 100;
                entry.level += 1;
                leveled = true;
            }
            if leveled {
                Some(format!("[ Your {} skill increases. ]", key))
            } else {
                None
            }
        }
        DialogueEffect::SetCounter { key, value } => {
            ch.achievement_counters
                .insert(key.clone(), (*value).max(0) as u32);
            None
        }
        DialogueEffect::IncrementCounter { key, by } => {
            let entry = ch.achievement_counters.entry(key.clone()).or_insert(0);
            let next = (*entry as i64) + (*by as i64);
            *entry = next.max(0) as u32;
            None
        }
        DialogueEffect::SetDgVar { scope, key, value } => {
            match scope {
                DgScope::Player => {
                    ch.dg_vars.insert(key.clone(), value.clone());
                    // Slice 2b: surface the write to the quest listener so a
                    // DgFlag objective tracking (key, value) can advance.
                    // Persist ch first (handle_dg_flag_set loads its own copy)
                    // and reload after so reward grants land back on `ch`.
                    let _ = db.save_character_data(ch.clone());
                    crate::quest::handle_dg_flag_set(db, connections, state, &ch.name, key, value);
                    if let Ok(Some(reloaded)) = db.get_character_data(&ch.name.to_lowercase()) {
                        *ch = reloaded;
                    }
                }
                DgScope::Mob => {
                    if let Some(mut mob_mut) = db.get_mobile_data(&mob.id).ok().flatten() {
                        mob_mut.dg_vars.insert(key.clone(), value.clone());
                        let _ = db.save_mobile_data(mob_mut);
                    }
                }
            }
            None
        }
        DialogueEffect::OfferQuest { vnum } => {
            let quest = match db.get_quest_data(vnum) {
                Ok(Some(q)) => q,
                _ => return Some(format!("[ unknown quest {} ]", vnum)),
            };
            if ch.active_quests.contains_key(&quest.vnum) {
                return Some(format!("[ Already on quest: {} ]", quest.name));
            }
            if ch.completed_quests.contains(&quest.vnum) && !quest.repeatable {
                return Some(format!("[ Already completed: {} ]", quest.name));
            }
            ch.active_quests.insert(
                quest.vnum.clone(),
                crate::types::ActiveQuest {
                    started_at: now_epoch_secs() as i64,
                    ..Default::default()
                },
            );
            Some(format!("\x1b[1;33m[ Quest accepted: {} ]\x1b[0m", quest.name))
        }
        DialogueEffect::AbandonQuest { vnum } => {
            let quest = db.get_quest_data(vnum).ok().flatten();
            if ch.active_quests.remove(vnum).is_some() {
                Some(format!(
                    "[ Abandoned: {} ]",
                    quest.map(|q| q.name).unwrap_or_else(|| vnum.clone())
                ))
            } else {
                Some(format!("[ Not on quest {} ]", vnum))
            }
        }
        DialogueEffect::SetQuestChoice {
            quest_vnum,
            key,
            value,
        } => {
            match ch.active_quests.get_mut(quest_vnum) {
                Some(aq) => {
                    aq.choice_vars.insert(key.clone(), value.clone());
                }
                None => {
                    tracing::warn!(
                        char = %ch.name,
                        quest_vnum = %quest_vnum,
                        key = %key,
                        "SetQuestChoice fired against a quest the player isn't on — no-op",
                    );
                }
            }
            None
        }
        DialogueEffect::CompleteQuest { vnum } => {
            // Persist any in-progress mutations on `ch` first so try_complete
            // (which loads its own copy) sees a consistent state. Then call the
            // canonical reward grantor and reload `ch` so the outer caller's
            // save doesn't overwrite the rewards.
            let _ = db.save_character_data(ch.clone());
            let completed = crate::quest::try_complete(db, connections, state, &ch.name, vnum);
            if let Ok(Some(reloaded)) = db.get_character_data(&ch.name.to_lowercase()) {
                *ch = reloaded;
            }
            if completed {
                None
            } else {
                let qname = db
                    .get_quest_data(vnum)
                    .ok()
                    .flatten()
                    .map(|q| q.name)
                    .unwrap_or_else(|| vnum.clone());
                if !ch.active_quests.contains_key(vnum) && !ch.completed_quests.contains(vnum) {
                    Some(format!("[ Not on quest: {} ]", qname))
                } else {
                    Some(format!("[ Quest not yet complete: {} ]", qname))
                }
            }
        }
        DialogueEffect::FireDgTrigger { trigger_type, arg } => {
            let trig_type = match parse_mob_trigger_type(trigger_type) {
                Some(t) => t,
                None => return Some(format!("[ unknown trigger type {} ]", trigger_type)),
            };
            // Re-fetch mob to capture any prior dg_var writes.
            if let Some(mob_now) = db.get_mobile_data(&mob.id).ok().flatten() {
                let conn_str = String::new();
                let db_arc = std::sync::Arc::new(db.clone());
                let conns_dummy: SharedConnections =
                    Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
                crate::script::dg::fire_mobile_dg_triggers(
                    &db_arc,
                    &conns_dummy,
                    &mob_now,
                    trig_type,
                    &conn_str,
                    "dialogue",
                    "dialogue",
                    arg,
                );
            }
            None
        }
    }
}

fn parse_mob_trigger_type(s: &str) -> Option<MobileTriggerType> {
    use crate::types::MobileTriggerType as T;
    match s.to_lowercase().as_str() {
        "on_greet" | "ongreet" => Some(T::OnGreet),
        "on_attack" | "onattack" => Some(T::OnAttack),
        "on_death" | "ondeath" => Some(T::OnDeath),
        "on_say" | "onsay" => Some(T::OnSay),
        "on_idle" | "onidle" => Some(T::OnIdle),
        "on_always" | "onalways" => Some(T::OnAlways),
        "on_flee" | "onflee" => Some(T::OnFlee),
        "on_fight" | "onfight" => Some(T::OnFight),
        "on_hit_percent" | "onhitpercent" => Some(T::OnHitPercent),
        "on_receive" | "onreceive" => Some(T::OnReceive),
        "on_bribe" | "onbribe" => Some(T::OnBribe),
        "on_load" | "onload" => Some(T::OnLoad),
        "on_command" | "oncommand" => Some(T::OnCommand),
        _ => None,
    }
}

fn is_flag_set(ch: &CharacterData, name: &str, scope: FlagScope, vnum: &str) -> bool {
    *ch.dialogue_flags
        .get(&flag_key(name, scope, vnum))
        .unwrap_or(&false)
}

fn flag_key(name: &str, scope: FlagScope, vnum: &str) -> String {
    match scope {
        FlagScope::Local => format!("{}:{}", vnum, name),
        FlagScope::Global => name.to_string(),
    }
}

fn count_inventory_vnum(db: &Db, char_name: &str, vnum: &str) -> i32 {
    let mut n = 0;
    if let Ok(items) = db.list_all_items() {
        for item in items {
            if item.is_prototype {
                continue;
            }
            if let Some(ref iv) = item.vnum {
                if iv != vnum {
                    continue;
                }
            } else {
                continue;
            }
            if let crate::types::ItemLocation::Inventory(ref name) = item.location {
                if name.eq_ignore_ascii_case(char_name) {
                    n += 1;
                }
            }
        }
    }
    n
}

fn find_inventory_item_uuid_by_vnum(db: &Db, char_name: &str, vnum: &str) -> Option<Uuid> {
    if let Ok(items) = db.list_all_items() {
        for item in items {
            if item.is_prototype {
                continue;
            }
            if let Some(ref iv) = item.vnum {
                if iv != vnum {
                    continue;
                }
            } else {
                continue;
            }
            if let crate::types::ItemLocation::Inventory(ref name) = item.location {
                if name.eq_ignore_ascii_case(char_name) {
                    return Some(item.id);
                }
            }
        }
    }
    None
}

fn validate_tree(tree: &DialogueTree) -> Result<(), String> {
    if tree.nodes.is_empty() {
        return Err("tree has no nodes".to_string());
    }
    if !tree.nodes.contains_key(&tree.root_node) {
        return Err(format!(
            "root_node `{}` is not in nodes map",
            tree.root_node
        ));
    }
    for (id, node) in &tree.nodes {
        for (ci, c) in node.choices.iter().enumerate() {
            if let DialogueTarget::Goto { node: target } = &c.target {
                if !tree.nodes.contains_key(target) {
                    return Err(format!(
                        "node `{}` choice {} (`{}`) targets missing node `{}`",
                        id,
                        ci + 1,
                        c.label,
                        target
                    ));
                }
            }
            if c.keyword.trim().is_empty() {
                return Err(format!(
                    "node `{}` choice {} has empty keyword",
                    id,
                    ci + 1
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;

    fn mk_tree() -> DialogueTree {
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "Hello.".to_string(),
                choices: vec![DialogueChoice {
                    keyword: "mayor".to_string(),
                    label: "About the mayor".to_string(),
                    target: DialogueTarget::Goto {
                        node: "mayor".to_string(),
                    },
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                }],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        nodes.insert(
            "mayor".to_string(),
            DialogueNode {
                text: "Mayor's sick.".to_string(),
                choices: vec![],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        DialogueTree {
            root_node: "root".to_string(),
            nodes,
        }
    }

    #[test]
    fn validate_passes_simple_tree() {
        assert!(validate_tree(&mk_tree()).is_ok());
    }

    #[test]
    fn validate_rejects_missing_root() {
        let mut t = mk_tree();
        t.root_node = "nope".to_string();
        assert!(validate_tree(&t).is_err());
    }

    #[test]
    fn validate_rejects_broken_goto() {
        let mut t = mk_tree();
        t.nodes.get_mut("root").unwrap().choices[0].target = DialogueTarget::Goto {
            node: "ghost".to_string(),
        };
        assert!(validate_tree(&t).is_err());
    }

    #[test]
    fn flag_key_local_includes_vnum() {
        assert_eq!(flag_key("asked", FlagScope::Local, "3001"), "3001:asked");
        assert_eq!(flag_key("asked", FlagScope::Global, "3001"), "asked");
    }

    fn make_character(name: &str) -> CharacterData {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": Uuid::nil(),
        }))
        .expect("build character")
    }

    fn open_temp_db(_label: &str) -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    /// Build dummy SharedConnections + SharedState for tests that don't need
    /// a live world. apply_effect signatures require both, but most condition/
    /// effect tests don't actually exercise CompleteQuest (the only effect
    /// that reads either).
    fn dummy_conns_and_state(db: &Db) -> (SharedConnections, SharedState) {
        use std::sync::{Arc, Mutex};
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let world = crate::World {
            engine: rhai::Engine::new(),
            db: db.clone(),
            connections: conns.clone(),
            scripts: HashMap::new(),
            command_metadata: HashMap::new(),
            class_definitions: HashMap::new(),
            trait_definitions: HashMap::new(),
            race_suggestions: Vec::new(),
            race_definitions: HashMap::new(),
            language_definitions: HashMap::new(),
            recipes: HashMap::new(),
            spell_definitions: HashMap::new(),
            achievement_definitions: HashMap::new(),
            achievement_index_by_counter: HashMap::new(),
            transports: HashMap::new(),
            chat_sender: None,
            shutdown_sender: None,
            shutdown_cancel_sender: None,
            ip_limiter: Arc::new(crate::ratelimit::IpRateLimiter::new()),
            command_throttle: Arc::new(crate::throttle::CommandThrottle::new()),
        };
        let state: SharedState = Arc::new(Mutex::new(world));
        (conns, state)
    }

    #[test]
    fn flag_set_and_unset_conditions_round_trip() {
        let mut ch = make_character("hero");
        let mut mob = MobileData::new("barkeep".into());
        mob.vnum = "3001".into();
        mob.dialogue_tree = Some(mk_tree());
        // Local flag round-trip.
        let local_set = DialogueCondition::FlagSet {
            name: "asked".into(),
            scope: FlagScope::Local,
        };
        let local_unset = DialogueCondition::FlagUnset {
            name: "asked".into(),
            scope: FlagScope::Local,
        };
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");
            let (conns, st) = dummy_conns_and_state(&db);
            assert!(!evaluate_condition(&local_set, &ch, &mob, &db));
            assert!(evaluate_condition(&local_unset, &ch, &mob, &db));
            // SetFlag effect makes condition true.
            let _ = apply_effect(
                &db,
                &conns,
                &st,
                &mut ch,
                &mob,
                &DialogueEffect::SetFlag {
                    name: "asked".into(),
                    scope: FlagScope::Local,
                },
            );
            assert!(evaluate_condition(&local_set, &ch, &mob, &db));
            assert!(!evaluate_condition(&local_unset, &ch, &mob, &db));
            assert!(ch.dialogue_flags.contains_key("3001:asked"));
            // ClearFlag inverts.
            let _ = apply_effect(
                &db,
                &conns,
                &st,
                &mut ch,
                &mob,
                &DialogueEffect::ClearFlag {
                    name: "asked".into(),
                    scope: FlagScope::Local,
                },
            );
            assert!(!ch.dialogue_flags.contains_key("3001:asked"));
            // Global scope: stored without prefix.
            let _ = apply_effect(
                &db,
                &conns,
                &st,
                &mut ch,
                &mob,
                &DialogueEffect::SetFlag {
                    name: "saved_village".into(),
                    scope: FlagScope::Global,
                },
            );
            assert!(ch.dialogue_flags.contains_key("saved_village"));
        }));
        result.unwrap();
    }

    #[test]
    fn skill_at_least_condition_reads_skills() {
        let mut ch = make_character("scholar");
        ch.skills.insert(
            "elvish".to_string(),
            crate::SkillProgress {
                level: 4,
                experience: 0,
            },
        );
        let mut mob = MobileData::new("guard".into());
        mob.vnum = "3002".into();
        mob.dialogue_tree = Some(mk_tree());
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let cond5 = DialogueCondition::SkillAtLeast {
                key: "elvish".into(),
                level: 5,
            };
            let cond4 = DialogueCondition::SkillAtLeast {
                key: "elvish".into(),
                level: 4,
            };
            assert!(!evaluate_condition(&cond5, &ch, &mob, &db));
            assert!(evaluate_condition(&cond4, &ch, &mob, &db));
        }));
        result.unwrap();
    }

    #[test]
    fn has_achievement_condition_reads_unlocks_map() {
        let mut ch = make_character("hero");
        let mut mob = MobileData::new("seneschal".into());
        mob.vnum = "3004".into();
        mob.dialogue_tree = Some(mk_tree());
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let cond = DialogueCondition::HasAchievement {
                key: "met_all_sires".into(),
            };
            assert!(!evaluate_condition(&cond, &ch, &mob, &db));
            ch.achievements_unlocked.insert(
                "met_all_sires".into(),
                crate::types::AchievementUnlock { unlocked_at: 1 },
            );
            assert!(evaluate_condition(&cond, &ch, &mob, &db));
        }));
        result.unwrap();
    }

    #[test]
    fn counter_at_least_condition_reads_achievement_counters() {
        let mut ch = make_character("hero");
        ch.achievement_counters.insert("quests.rats".into(), 5);
        let mut mob = MobileData::new("foreman".into());
        mob.vnum = "3003".into();
        mob.dialogue_tree = Some(mk_tree());
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let lo = DialogueCondition::CounterAtLeast {
                key: "quests.rats".into(),
                value: 5,
            };
            let hi = DialogueCondition::CounterAtLeast {
                key: "quests.rats".into(),
                value: 6,
            };
            assert!(evaluate_condition(&lo, &ch, &mob, &db));
            assert!(!evaluate_condition(&hi, &ch, &mob, &db));
        }));
        result.unwrap();
    }

    #[test]
    fn dg_var_equals_condition_reads_player_and_mob_vars() {
        let mut ch = make_character("hero");
        ch.dg_vars.insert("faction".into(), "guild".into());
        let mut mob = MobileData::new("agent".into());
        mob.vnum = "3004".into();
        mob.dg_vars.insert("on_duty".into(), "1".into());
        mob.dialogue_tree = Some(mk_tree());
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let player_match = DialogueCondition::DgVarEquals {
                scope: DgScope::Player,
                key: "faction".into(),
                value: "guild".into(),
            };
            let player_miss = DialogueCondition::DgVarEquals {
                scope: DgScope::Player,
                key: "faction".into(),
                value: "rival".into(),
            };
            let mob_match = DialogueCondition::DgVarEquals {
                scope: DgScope::Mob,
                key: "on_duty".into(),
                value: "1".into(),
            };
            assert!(evaluate_condition(&player_match, &ch, &mob, &db));
            assert!(!evaluate_condition(&player_miss, &ch, &mob, &db));
            assert!(evaluate_condition(&mob_match, &ch, &mob, &db));
        }));
        result.unwrap();
    }

    #[test]
    fn counter_effects_set_and_increment() {
        let mut ch = make_character("hero");
        let mut mob = MobileData::new("clerk".into());
        mob.vnum = "3005".into();
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let (conns, st) = dummy_conns_and_state(&db);
            let _ = apply_effect(
                &db,
                &conns,
                &st,
                &mut ch,
                &mob,
                &DialogueEffect::SetCounter {
                    key: "shipments.delivered".into(),
                    value: 3,
                },
            );
            assert_eq!(ch.achievement_counters.get("shipments.delivered"), Some(&3));
            let _ = apply_effect(
                &db,
                &conns,
                &st,
                &mut ch,
                &mob,
                &DialogueEffect::IncrementCounter {
                    key: "shipments.delivered".into(),
                    by: 2,
                },
            );
            assert_eq!(ch.achievement_counters.get("shipments.delivered"), Some(&5));
        }));
        result.unwrap();
    }

    #[test]
    fn visible_choices_filter_by_condition() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "?".into(),
                choices: vec![
                    DialogueChoice {
                        keyword: "always".into(),
                        label: "Always shown".into(),
                        target: DialogueTarget::Exit,
                        conditions: vec![],
                        effects: vec![],
                        hint: None,
                        cooldown_secs: None,
                        once_per_player: false,
                    },
                    DialogueChoice {
                        keyword: "gated".into(),
                        label: "Only after asked".into(),
                        target: DialogueTarget::Exit,
                        conditions: vec![DialogueCondition::FlagSet {
                            name: "asked".into(),
                            scope: FlagScope::Local,
                        }],
                        effects: vec![],
                        hint: None,
                        cooldown_secs: None,
                        once_per_player: false,
                    },
                ],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        let tree = DialogueTree {
            root_node: "root".into(),
            nodes,
        };
        let ch = make_character("hero");
        let mut mob = MobileData::new("vendor".into());
        mob.vnum = "3006".into();
        mob.dialogue_tree = Some(tree);
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let node = &mob.dialogue_tree.as_ref().unwrap().nodes["root"];
            let visible = visible_choices(node, &ch, &mob, &db);
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].keyword, "always");
            // After setting flag, gated choice appears.
            let mut ch2 = ch.clone();
            ch2.dialogue_flags.insert("3006:asked".into(), true);
            let visible2 = visible_choices(node, &ch2, &mob, &db);
            assert_eq!(visible2.len(), 2);
        }));
        result.unwrap();
    }

    #[test]
    fn current_node_for_falls_back_to_root() {
        let mut mob = MobileData::new("npc".into());
        mob.vnum = "3007".into();
        mob.dialogue_tree = Some(mk_tree());
        let ch = make_character("hero");
        assert_eq!(current_node_for(&ch, &mob), "root");
        // Set explicit cursor.
        let mut ch2 = ch.clone();
        ch2.dialogue_pair_state.insert(
            "3007".into(),
            DialoguePairState {
                current_node: Some("mayor".into()),
                last_seen_secs: 0,
                visit_counts: std::collections::HashMap::new(),
                choice_cooldowns: HashMap::new(),
                choices_picked_once: std::collections::HashSet::new(),
            },
        );
        assert_eq!(current_node_for(&ch2, &mob), "mayor");
        // Ghost cursor falls back to root.
        let mut ch3 = ch.clone();
        ch3.dialogue_pair_state.insert(
            "3007".into(),
            DialoguePairState {
                current_node: Some("ghost".into()),
                last_seen_secs: 0,
                visit_counts: std::collections::HashMap::new(),
                choice_cooldowns: HashMap::new(),
                choices_picked_once: std::collections::HashSet::new(),
            },
        );
        assert_eq!(current_node_for(&ch3, &mob), "root");
    }

    #[test]
    fn keyword_and_movement_classifiers() {
        assert!(is_movement_direction("n"));
        assert!(is_movement_direction("north"));
        assert!(is_movement_direction("ne"));
        assert!(is_movement_direction("up"));
        assert!(!is_movement_direction("nope"));
        assert!(!is_movement_direction("mayor"));
    }

    #[test]
    fn round_trips_full_tree_through_json() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "Greetings.".to_string(),
                choices: vec![DialogueChoice {
                    keyword: "quest".to_string(),
                    label: "I'll take it".to_string(),
                    target: DialogueTarget::Goto {
                        node: "given".to_string(),
                    },
                    conditions: vec![
                        DialogueCondition::FlagUnset {
                            name: "did_quest".to_string(),
                            scope: FlagScope::Local,
                        },
                        DialogueCondition::SkillAtLeast {
                            key: "elvish".to_string(),
                            level: 5,
                        },
                    ],
                    effects: vec![
                        DialogueEffect::SetFlag {
                            name: "did_quest".to_string(),
                            scope: FlagScope::Local,
                        },
                        DialogueEffect::GiveItem {
                            vnum: "5023".to_string(),
                            qty: 1,
                        },
                        DialogueEffect::AwardSkillXp {
                            skill: "diplomacy".to_string(),
                            amount: 50,
                        },
                        DialogueEffect::FireDgTrigger {
                            trigger_type: "on_receive".to_string(),
                            arg: String::new(),
                        },
                    ],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                }],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        nodes.insert(
            "given".to_string(),
            DialogueNode {
                text: "Done.".to_string(),
                choices: vec![DialogueChoice {
                    keyword: "bye".to_string(),
                    label: "Goodbye".to_string(),
                    target: DialogueTarget::Exit,
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                }],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        let t = DialogueTree {
            root_node: "root".to_string(),
            nodes,
        };
        let json = serde_json::to_string(&t).expect("serialize");
        let back: DialogueTree = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.root_node, t.root_node);
        assert!(back.nodes.contains_key("root"));
        assert!(back.nodes.contains_key("given"));
        assert_eq!(back.nodes["given"].choices[0].keyword, "bye");
    }

    #[test]
    fn on_enter_fires_only_on_first_visit_and_on_each_visit_fires_every_time() {
        // Tree with a single "shop" node whose on_enter sets a counter to 1
        // and whose on_each_visit increments a different counter.
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "Welcome.".into(),
                choices: vec![
                    DialogueChoice {
                        keyword: "shop".into(),
                        label: "Visit the shop".into(),
                        target: DialogueTarget::Goto {
                            node: "shop".into(),
                        },
                        conditions: vec![],
                        effects: vec![],
                        hint: None,
                        cooldown_secs: None,
                        once_per_player: false,
                    },
                    DialogueChoice {
                        keyword: "leave".into(),
                        label: "Back".into(),
                        target: DialogueTarget::Goto {
                            node: "root".into(),
                        },
                        conditions: vec![],
                        effects: vec![],
                        hint: None,
                        cooldown_secs: None,
                        once_per_player: false,
                    },
                ],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        nodes.insert(
            "shop".to_string(),
            DialogueNode {
                text: "Wares.".into(),
                choices: vec![DialogueChoice {
                    keyword: "back".into(),
                    label: "Back".into(),
                    target: DialogueTarget::Goto {
                        node: "root".into(),
                    },
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                }],
                on_enter: vec![DialogueEffect::SetCounter {
                    key: "shop.first_visit".into(),
                    value: 1,
                }],
                on_each_visit: vec![DialogueEffect::IncrementCounter {
                    key: "shop.visits".into(),
                    by: 1,
                }],
                on_exit: vec![DialogueEffect::IncrementCounter {
                    key: "shop.exits".into(),
                    by: 1,
                }],
            },
        );
        let mut mob = MobileData::new("vendor".into());
        mob.vnum = "9000".into();
        mob.dialogue_tree = Some(DialogueTree {
            root_node: "root".into(),
            nodes,
        });
        let mut ch = make_character("hero");
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let (conns, st) = dummy_conns_and_state(&db);
            let go_to_shop = DialogueChoice {
                keyword: "shop".into(),
                label: "Visit the shop".into(),
                target: DialogueTarget::Goto { node: "shop".into() },
                conditions: vec![],
                effects: vec![],
                hint: None,
                cooldown_secs: None,
                once_per_player: false,
            };
            let back_to_root = DialogueChoice {
                keyword: "back".into(),
                label: "Back".into(),
                target: DialogueTarget::Goto { node: "root".into() },
                conditions: vec![],
                effects: vec![],
                hint: None,
                cooldown_secs: None,
                once_per_player: false,
            };
            // Visit shop first time: on_enter + on_each_visit fire.
            take_choice(&db, &conns, &st, &mut ch, &mob, &go_to_shop);
            assert_eq!(ch.achievement_counters.get("shop.first_visit"), Some(&1));
            assert_eq!(ch.achievement_counters.get("shop.visits"), Some(&1));
            // Leaving shop fires on_exit.
            take_choice(&db, &conns, &st, &mut ch, &mob, &back_to_root);
            assert_eq!(ch.achievement_counters.get("shop.exits"), Some(&1));
            // Visit shop second time: on_enter must NOT re-fire (still 1);
            // on_each_visit increments to 2.
            take_choice(&db, &conns, &st, &mut ch, &mob, &go_to_shop);
            assert_eq!(ch.achievement_counters.get("shop.first_visit"), Some(&1));
            assert_eq!(ch.achievement_counters.get("shop.visits"), Some(&2));
            // Visit counter on the pair-state matches.
            let visits = ch
                .dialogue_pair_state
                .get("9000")
                .and_then(|s| s.visit_counts.get("shop"))
                .copied()
                .unwrap_or(0);
            assert_eq!(visits, 2);
        }));
        result.unwrap();
    }

    #[test]
    fn exit_target_fires_on_exit_for_current_node() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "Hi.".into(),
                choices: vec![DialogueChoice {
                    keyword: "bye".into(),
                    label: "Goodbye".into(),
                    target: DialogueTarget::Exit,
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                }],
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![DialogueEffect::SetFlag {
                    name: "said_goodbye".into(),
                    scope: FlagScope::Local,
                }],
            },
        );
        let mut mob = MobileData::new("guide".into());
        mob.vnum = "9001".into();
        mob.dialogue_tree = Some(DialogueTree {
            root_node: "root".into(),
            nodes,
        });
        let mut ch = make_character("hero");
        // Place player at root manually to skip start_talk.
        ch.dialogue_pair_state.insert(
            "9001".into(),
            DialoguePairState {
                current_node: Some("root".into()),
                last_seen_secs: 0,
                visit_counts: std::collections::HashMap::new(),
                choice_cooldowns: HashMap::new(),
                choices_picked_once: std::collections::HashSet::new(),
            },
        );
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let (conns, st) = dummy_conns_and_state(&db);
            let bye = DialogueChoice {
                keyword: "bye".into(),
                label: "Goodbye".into(),
                target: DialogueTarget::Exit,
                conditions: vec![],
                effects: vec![],
                hint: None,
                cooldown_secs: None,
                once_per_player: false,
            };
            let (_, _, finished) = take_choice(&db, &conns, &st, &mut ch, &mob, &bye);
            assert!(finished);
            assert_eq!(ch.dialogue_flags.get("9001:said_goodbye"), Some(&true));
        }));
        result.unwrap();
    }

    #[test]
    fn repeat_target_does_not_fire_entry_or_exit_triggers() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "Repeat?".into(),
                choices: vec![DialogueChoice {
                    keyword: "again".into(),
                    label: "Repeat".into(),
                    target: DialogueTarget::Repeat,
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                }],
                on_enter: vec![DialogueEffect::IncrementCounter {
                    key: "enters".into(),
                    by: 1,
                }],
                on_each_visit: vec![DialogueEffect::IncrementCounter {
                    key: "visits".into(),
                    by: 1,
                }],
                on_exit: vec![DialogueEffect::IncrementCounter {
                    key: "exits".into(),
                    by: 1,
                }],
            },
        );
        let mut mob = MobileData::new("loop".into());
        mob.vnum = "9002".into();
        mob.dialogue_tree = Some(DialogueTree {
            root_node: "root".into(),
            nodes,
        });
        let mut ch = make_character("hero");
        ch.dialogue_pair_state.insert(
            "9002".into(),
            DialoguePairState {
                current_node: Some("root".into()),
                last_seen_secs: 0,
                visit_counts: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("root".to_string(), 1u32);
                    m
                },
                choice_cooldowns: HashMap::new(),
                choices_picked_once: std::collections::HashSet::new(),
            },
        );
        let temp = tempfile::tempdir().expect("create temp dir");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = Db::open(temp.path()).expect("open db");

            let (conns, st) = dummy_conns_and_state(&db);
            let again = DialogueChoice {
                keyword: "again".into(),
                label: "Repeat".into(),
                target: DialogueTarget::Repeat,
                conditions: vec![],
                effects: vec![],
                hint: None,
                cooldown_secs: None,
                once_per_player: false,
            };
            take_choice(&db, &conns, &st, &mut ch, &mob, &again);
            // None of the trigger sets fired on Repeat.
            assert!(ch.achievement_counters.get("enters").is_none());
            assert!(ch.achievement_counters.get("visits").is_none());
            assert!(ch.achievement_counters.get("exits").is_none());
        }));
        result.unwrap();
    }

    /// Helper for slice 3 tests: build a one-node tree whose root has the
    /// given choices and stamp it onto the mob.
    fn mk_mob_with_choices(vnum: &str, choices: Vec<DialogueChoice>) -> MobileData {
        let mut nodes = HashMap::new();
        nodes.insert(
            "root".to_string(),
            DialogueNode {
                text: "Hi.".into(),
                choices,
                on_enter: vec![],
                on_each_visit: vec![],
                on_exit: vec![],
            },
        );
        let mut mob = MobileData::new("npc".into());
        mob.vnum = vnum.to_string();
        mob.dialogue_tree = Some(DialogueTree {
            root_node: "root".into(),
            nodes,
        });
        mob
    }

    #[test]
    fn classify_locked_with_hint_surfaces_in_menu() {
        let ch = make_character("hero");
        let mob = mk_mob_with_choices(
            "9301",
            vec![DialogueChoice {
                keyword: "smith".into(),
                label: "Ask about smithing".into(),
                target: DialogueTarget::Repeat,
                conditions: vec![DialogueCondition::FlagSet {
                    name: "knows_smith".into(),
                    scope: FlagScope::Local,
                }],
                effects: vec![],
                hint: Some("She gauges your hands — you don't look the type.".into()),
                cooldown_secs: None,
                once_per_player: false,
            }],
        );
        let (db, _temp) = open_temp_db(
"classify_locked_hint");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let node = mob.dialogue_tree.as_ref().unwrap().nodes.get("root").unwrap();
            let classified = classify_choices("root", node, &ch, &mob, &db, 1_000);
            assert_eq!(classified.len(), 1);
            match &classified[0].visibility {
                ChoiceVisibility::Locked { hint } => {
                    assert!(hint.contains("gauges your hands"));
                }
                other => panic!("expected Locked, got {:?}", other),
            }
            let menu = render_classified_menu(&classified);
            assert!(menu.contains("(?)"), "menu line should mark locked: {}", menu);
            assert!(menu.contains("gauges your hands"));
        }));
        result.unwrap();
    }

    #[test]
    fn classify_locked_without_hint_is_silently_hidden() {
        let ch = make_character("hero");
        let mob = mk_mob_with_choices(
            "9302",
            vec![DialogueChoice {
                keyword: "smith".into(),
                label: "Ask about smithing".into(),
                target: DialogueTarget::Repeat,
                conditions: vec![DialogueCondition::FlagSet {
                    name: "knows_smith".into(),
                    scope: FlagScope::Local,
                }],
                effects: vec![],
                hint: None,
                cooldown_secs: None,
                once_per_player: false,
            }],
        );
        let (db, _temp) = open_temp_db(
"classify_no_hint_hidden");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let node = mob.dialogue_tree.as_ref().unwrap().nodes.get("root").unwrap();
            let classified = classify_choices("root", node, &ch, &mob, &db, 1_000);
            assert!(
                classified.is_empty(),
                "no-hint locked choice must drop from output"
            );
            let menu = render_classified_menu(&classified);
            assert_eq!(menu, "  bye. (leave)");
        }));
        result.unwrap();
    }

    #[test]
    fn classify_cooldown_blocks_then_clears_after_elapsed() {
        let mut ch = make_character("hero");
        let mob = mk_mob_with_choices(
            "9303",
            vec![DialogueChoice {
                keyword: "rumor".into(),
                label: "Press for rumors".into(),
                target: DialogueTarget::Repeat,
                conditions: vec![],
                effects: vec![],
                hint: None,
                cooldown_secs: Some(60),
                once_per_player: false,
            }],
        );
        let (db, _temp) = open_temp_db(
"classify_cd_blocks");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let node = mob
                .dialogue_tree
                .as_ref()
                .unwrap()
                .nodes
                .get("root")
                .unwrap()
                .clone();
            // Stamp last-pick at t=1000 directly to avoid time skew on take_choice.
            let pair = ch
                .dialogue_pair_state
                .entry(mob.vnum.clone())
                .or_insert_with(DialoguePairState::default);
            pair.choice_cooldowns
                .insert(cooldown_key("root", "rumor"), 1_000);

            // 30s later — still cooling.
            let classified = classify_choices("root", &node, &ch, &mob, &db, 1_030);
            assert_eq!(classified.len(), 1);
            match classified[0].visibility {
                ChoiceVisibility::Cooldown { remaining_secs } => {
                    assert_eq!(remaining_secs, 30);
                }
                ref other => panic!("expected Cooldown, got {:?}", other),
            }
            let menu = render_classified_menu(&classified);
            assert!(menu.contains("(available in"), "got: {}", menu);

            // 65s later — fully elapsed.
            let classified = classify_choices("root", &node, &ch, &mob, &db, 1_065);
            assert_eq!(classified.len(), 1);
            assert_eq!(classified[0].visibility, ChoiceVisibility::Available);
        }));
        result.unwrap();
    }

    #[test]
    fn classify_once_per_player_disappears_after_first_pick() {
        let mut ch = make_character("hero");
        let mob = mk_mob_with_choices(
            "9304",
            vec![DialogueChoice {
                keyword: "gift".into(),
                label: "Accept the heirloom".into(),
                target: DialogueTarget::Exit,
                conditions: vec![],
                effects: vec![],
                hint: None,
                cooldown_secs: None,
                once_per_player: true,
            }],
        );
        let (db, _temp) = open_temp_db(
"classify_once");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let node = mob
                .dialogue_tree
                .as_ref()
                .unwrap()
                .nodes
                .get("root")
                .unwrap()
                .clone();
            // First classify: visible, available.
            let classified = classify_choices("root", &node, &ch, &mob, &db, 100);
            assert_eq!(classified.len(), 1);
            assert_eq!(classified[0].visibility, ChoiceVisibility::Available);

            // Simulate the pick by recording it.
            record_choice_pick(&mut ch, &mob.vnum, "root", &node.choices[0]);
            let key = cooldown_key("root", "gift");
            assert!(ch
                .dialogue_pair_state
                .get(&mob.vnum)
                .unwrap()
                .choices_picked_once
                .contains(&key));

            // Second classify: gone from output.
            let classified = classify_choices("root", &node, &ch, &mob, &db, 100);
            assert!(
                classified.is_empty(),
                "once-picked choice must drop from output"
            );
        }));
        result.unwrap();
    }
}

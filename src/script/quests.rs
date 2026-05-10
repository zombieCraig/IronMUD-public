//! Rhai bindings for the quest system. Registers `QuestData` as a Rhai type
//! and exposes builder/player ops as free fns. The heavy lifting lives in
//! `crate::quest`; this layer is the .rhai surface.

use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, Map};

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::types::{ActiveQuest, QuestData, QuestObjective, QuestReward};

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // Register QuestData with property getters used by player commands.
    engine
        .register_type_with_name::<QuestData>("QuestData")
        .register_get("vnum", |q: &mut QuestData| q.vnum.clone())
        .register_get("name", |q: &mut QuestData| q.name.clone())
        .register_get("summary", |q: &mut QuestData| q.summary.clone())
        .register_get("description", |q: &mut QuestData| q.description.clone())
        .register_get("completion_text", |q: &mut QuestData| q.completion_text.clone())
        .register_get("repeatable", |q: &mut QuestData| q.repeatable)
        .register_get("giver_mob_vnum", |q: &mut QuestData| {
            q.giver_mob_vnum.clone().unwrap_or_default()
        })
        .register_get("prereq_quest_vnum", |q: &mut QuestData| {
            q.prereq_quest_vnum.clone().unwrap_or_default()
        })
        .register_get("min_player_skill_total", |q: &mut QuestData| {
            q.min_player_skill_total.unwrap_or(0) as i64
        })
        .register_get("duration_secs", |q: &mut QuestData| q.duration_secs.unwrap_or(0))
        .register_get("keywords", |q: &mut QuestData| {
            q.keywords
                .iter()
                .map(|s| Dynamic::from(s.clone()))
                .collect::<Array>()
        });

    // get_quest_data(vnum) -> QuestData | ()
    {
        let cloned_db = db.clone();
        engine.register_fn("get_quest_data", move |vnum: String| -> Dynamic {
            match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => Dynamic::from(q),
                _ => Dynamic::UNIT,
            }
        });
    }

    // list_all_quests() -> Array<QuestData>
    {
        let cloned_db = db.clone();
        engine.register_fn("list_all_quests", move || -> Array {
            cloned_db
                .list_all_quests()
                .unwrap_or_default()
                .into_iter()
                .map(Dynamic::from)
                .collect()
        });
    }

    // create_quest(vnum, name) -> String — "" on success, error otherwise.
    {
        let cloned_db = db.clone();
        engine.register_fn("create_quest", move |vnum: String, name: String| -> String {
            if vnum.trim().is_empty() {
                return "vnum required".to_string();
            }
            if name.trim().is_empty() {
                return "name required".to_string();
            }
            if matches!(cloned_db.get_quest_data(&vnum), Ok(Some(_))) {
                return format!("quest `{}` already exists", vnum);
            }
            let q = QuestData::new(vnum, name);
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }

    // delete_quest(vnum) -> bool
    {
        let cloned_db = db.clone();
        engine.register_fn("delete_quest", move |vnum: String| -> bool {
            cloned_db.delete_quest(&vnum).is_ok()
        });
    }

    // String-field setters: name, summary, description, completion_text.
    {
        let cloned_db = db.clone();
        engine.register_fn("set_quest_name", move |vnum: String, value: String| -> String {
            let mut q = match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => q,
                _ => return format!("no such quest `{}`", vnum),
            };
            q.name = value;
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }
    {
        let cloned_db = db.clone();
        engine.register_fn("set_quest_summary", move |vnum: String, value: String| -> String {
            let mut q = match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => q,
                _ => return format!("no such quest `{}`", vnum),
            };
            q.summary = value;
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }
    {
        let cloned_db = db.clone();
        engine.register_fn("set_quest_description", move |vnum: String, value: String| -> String {
            let mut q = match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => q,
                _ => return format!("no such quest `{}`", vnum),
            };
            q.description = value;
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "set_quest_completion_text",
            move |vnum: String, value: String| -> String {
                let mut q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return format!("no such quest `{}`", vnum),
                };
                q.completion_text = value;
                if let Err(e) = cloned_db.save_quest_data(&q) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // set_quest_giver(vnum, mob_vnum) — empty mob_vnum clears.
    {
        let cloned_db = db.clone();
        engine.register_fn("set_quest_giver", move |vnum: String, mob_vnum: String| -> String {
            let mut q = match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => q,
                _ => return format!("no such quest `{}`", vnum),
            };
            q.giver_mob_vnum = if mob_vnum.trim().is_empty() {
                None
            } else {
                Some(mob_vnum)
            };
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }

    // set_quest_repeatable(vnum, bool) -> String
    {
        let cloned_db = db.clone();
        engine.register_fn("set_quest_repeatable", move |vnum: String, on: bool| -> String {
            let mut q = match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => q,
                _ => return format!("no such quest `{}`", vnum),
            };
            q.repeatable = on;
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }

    // set_quest_keywords(vnum, "kw1 kw2") -> String — space-separated, replaces.
    {
        let cloned_db = db.clone();
        engine.register_fn("set_quest_keywords", move |vnum: String, kws: String| -> String {
            let mut q = match cloned_db.get_quest_data(&vnum) {
                Ok(Some(q)) => q,
                _ => return format!("no such quest `{}`", vnum),
            };
            q.keywords = kws
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            if let Err(e) = cloned_db.save_quest_data(&q) {
                return format!("save error: {}", e);
            }
            String::new()
        });
    }

    // set_quest_prereq(vnum, prereq_vnum) -> String — empty/clear/none clears.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "set_quest_prereq",
            move |vnum: String, prereq_vnum: String| -> String {
                let mut q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return format!("no such quest `{}`", vnum),
                };
                let trimmed = prereq_vnum.trim();
                q.prereq_quest_vnum = if trimmed.is_empty()
                    || trimmed.eq_ignore_ascii_case("clear")
                    || trimmed.eq_ignore_ascii_case("none")
                {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                if let Err(e) = cloned_db.save_quest_data(&q) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // set_quest_min_skill(vnum, n) -> String — 0 or negative clears.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "set_quest_min_skill",
            move |vnum: String, n: i64| -> String {
                let mut q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return format!("no such quest `{}`", vnum),
                };
                q.min_player_skill_total = if n <= 0 { None } else { Some(n as i32) };
                if let Err(e) = cloned_db.save_quest_data(&q) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // set_quest_duration(vnum, secs) -> String — 0 or negative clears (no expiry).
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "set_quest_duration",
            move |vnum: String, secs: i64| -> String {
                let mut q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return format!("no such quest `{}`", vnum),
                };
                q.duration_secs = if secs <= 0 { None } else { Some(secs) };
                if let Err(e) = cloned_db.save_quest_data(&q) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // === Objective add/remove ===
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_objective_kill",
            move |vnum: String, mob_vnum: String, count: i64| -> String {
                push_objective(&cloned_db, &vnum, QuestObjective::KillMob {
                    vnum: mob_vnum,
                    count: count.max(1) as i32,
                })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_objective_bring",
            move |vnum: String, item_vnum: String, qty: i64, return_mob: String| -> String {
                let return_to = if return_mob.trim().is_empty() {
                    None
                } else {
                    Some(return_mob)
                };
                push_objective(&cloned_db, &vnum, QuestObjective::BringItem {
                    vnum: item_vnum,
                    qty: qty.max(1) as i32,
                    return_to_mob_vnum: return_to,
                })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_objective_visit",
            move |vnum: String, room_vnum: String| -> String {
                push_objective(&cloned_db, &vnum, QuestObjective::VisitRoom { vnum: room_vnum })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_objective_flag",
            move |vnum: String, var: String, value: String| -> String {
                push_objective(&cloned_db, &vnum, QuestObjective::DgFlag { var, value })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "remove_quest_objective",
            move |vnum: String, idx: i64| -> String {
                let mut q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return format!("no such quest `{}`", vnum),
                };
                if idx < 0 || (idx as usize) >= q.objectives.len() {
                    return format!("objective index out of range: {}", idx);
                }
                q.objectives.remove(idx as usize);
                if let Err(e) = cloned_db.save_quest_data(&q) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // === Reward add/remove ===
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_reward_gold",
            move |vnum: String, amount: i64| -> String {
                push_reward(&cloned_db, &vnum, QuestReward::Gold { amount })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_reward_item",
            move |vnum: String, item_vnum: String, qty: i64| -> String {
                push_reward(&cloned_db, &vnum, QuestReward::Item {
                    vnum: item_vnum,
                    qty: qty.max(1) as i32,
                })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_reward_skill",
            move |vnum: String, skill: String, amount: i64| -> String {
                push_reward(&cloned_db, &vnum, QuestReward::SkillXp {
                    skill,
                    amount: amount as i32,
                })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_reward_achievement",
            move |vnum: String, key: String| -> String {
                push_reward(&cloned_db, &vnum, QuestReward::Achievement { key })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_reward_recipe",
            move |vnum: String, recipe_id: String| -> String {
                push_reward(&cloned_db, &vnum, QuestReward::LearnRecipe { recipe_id })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "add_quest_reward_embrace_clan",
            move |vnum: String, clan: String| -> String {
                let trimmed = clan.trim().to_lowercase();
                if trimmed.is_empty() {
                    return "clan id cannot be empty".to_string();
                }
                push_reward(&cloned_db, &vnum, QuestReward::EmbraceClan { clan: trimmed })
            },
        );
    }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "remove_quest_reward",
            move |vnum: String, idx: i64| -> String {
                let mut q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return format!("no such quest `{}`", vnum),
                };
                if idx < 0 || (idx as usize) >= q.rewards.len() {
                    return format!("reward index out of range: {}", idx);
                }
                q.rewards.remove(idx as usize);
                if let Err(e) = cloned_db.save_quest_data(&q) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // === Player-facing ops ===

    // quest_offer(player_name, vnum) -> String ("" success, error otherwise)
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "quest_offer",
            move |player_name: String, vnum: String| -> String {
                crate::quest::offer(&cloned_db, &player_name, &vnum)
            },
        );
    }

    // quest_abandon(player_name, vnum) -> String
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "quest_abandon",
            move |player_name: String, vnum: String| -> String {
                crate::quest::abandon(&cloned_db, &player_name, &vnum)
            },
        );
    }

    // quest_try_complete(player_name, vnum) -> bool — used by player's
    // explicit "turn in" path or by listener follow-ups.
    {
        let cloned_db = db.clone();
        let cloned_conns = connections.clone();
        let cloned_state = state.clone();
        engine.register_fn(
            "quest_try_complete",
            move |player_name: String, vnum: String| -> bool {
                crate::quest::try_complete(
                    &cloned_db,
                    &cloned_conns,
                    &cloned_state,
                    &player_name,
                    &vnum,
                )
            },
        );
    }

    // quest_handle_item_to_mob(giver_name, mob_id, item_id) -> bool
    // Called from give.rhai BEFORE the existing on_receive trigger fires.
    // True means the quest consumed the item — caller must skip on_receive
    // and not re-place the item on the mob.
    {
        let cloned_db = db.clone();
        let cloned_conns = connections.clone();
        let cloned_state = state.clone();
        engine.register_fn(
            "quest_handle_item_to_mob",
            move |giver_name: String, mob_id: String, item_id: String| -> bool {
                let mob_uuid = match uuid::Uuid::parse_str(&mob_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let mob = match cloned_db.get_mobile_data(&mob_uuid) {
                    Ok(Some(m)) => m,
                    _ => return false,
                };
                let item = match cloned_db.get_item_data(&item_uuid) {
                    Ok(Some(i)) => i,
                    _ => return false,
                };
                crate::quest::handle_item_to_mob(
                    &cloned_db,
                    &cloned_conns,
                    &cloned_state,
                    &giver_name,
                    &mob,
                    &item,
                )
            },
        );
    }

    // record_mob_damaged_by(mobile_id, char_name, dmg) -> bool
    // Accumulate per-fight damage attribution on a mob's CombatState. Used by
    // script-side damage paths (cast_damage, etc.) so handle_mob_kill from the
    // combat tick credits all contributors (slice 3c party credit).
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "record_mob_damaged_by",
            move |mobile_id: String, char_name: String, dmg: i64| -> bool {
                if dmg <= 0 || char_name.is_empty() {
                    return false;
                }
                let mid = match uuid::Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let mut mob = match cloned_db.get_mobile_data(&mid) {
                    Ok(Some(m)) => m,
                    _ => return false,
                };
                *mob.combat
                    .damaged_by
                    .entry(char_name.to_lowercase())
                    .or_insert(0) += dmg as i32;
                cloned_db.save_mobile_data(mob).is_ok()
            },
        );
    }

    // quest_credit_mob_kill(killer_name, mob_proto_vnum) -> bool
    // Single-killer convenience used from script-side kill sites (bash,
    // backstab, etc.) that don't share the combat tick's damaged_by map.
    // Returns true (always — the underlying handle_mob_kill is best-effort).
    {
        let cloned_db = db.clone();
        let cloned_conns = connections.clone();
        let cloned_state = state.clone();
        engine.register_fn(
            "quest_credit_mob_kill",
            move |killer_name: String, mob_proto_vnum: String| -> bool {
                let damaged_by: std::collections::HashMap<String, i32> =
                    std::collections::HashMap::new();
                crate::quest::handle_mob_kill(
                    &cloned_db,
                    &cloned_conns,
                    &cloned_state,
                    &killer_name,
                    &mob_proto_vnum,
                    &damaged_by,
                );
                true
            },
        );
    }

    // quest_handle_room_visit(player_name, room_vnum) -> bool
    // Called from go.rhai immediately after display_room. Advances any
    // VisitRoom objective whose vnum matches; auto-completes visit-only
    // quests. Returns true when at least one quest progressed (info only;
    // go.rhai discards the result today).
    {
        let cloned_db = db.clone();
        let cloned_conns = connections.clone();
        let cloned_state = state.clone();
        engine.register_fn(
            "quest_handle_room_visit",
            move |player_name: String, room_vnum: String| -> bool {
                crate::quest::handle_room_visit(
                    &cloned_db,
                    &cloned_conns,
                    &cloned_state,
                    &player_name,
                    &room_vnum,
                )
            },
        );
    }

    // quest_handle_dg_flag_set(player_name, var, value) -> bool
    // Called from src/script/dg/eval.rs after a character-scoped DG var write.
    // Advances any DgFlag objective whose (var, value) matches; auto-completes
    // flag-only quests.
    {
        let cloned_db = db.clone();
        let cloned_conns = connections.clone();
        let cloned_state = state.clone();
        engine.register_fn(
            "quest_handle_dg_flag_set",
            move |player_name: String, var: String, value: String| -> bool {
                crate::quest::handle_dg_flag_set(
                    &cloned_db,
                    &cloned_conns,
                    &cloned_state,
                    &player_name,
                    &var,
                    &value,
                )
            },
        );
    }

    // describe_quest_offers(viewer_name, mob_vnum) -> String
    // Returns "(awaits your return)" / "(has a quest for you)" / "" cue
    // for use in look + examine. Empty string means no cue.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "describe_quest_offers",
            move |viewer_name: String, mob_vnum: String| -> String {
                crate::quest::describe_quest_offers(&cloned_db, &viewer_name, &mob_vnum)
                    .unwrap_or_default()
            },
        );
    }

    // quest_format_progress(player_name) -> Array<Map>
    // Each map: { vnum, name, summary, lines: [ String, ... ] }
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "quest_format_progress",
            move |player_name: String| -> Array {
                crate::quest::format_progress(&cloned_db, &player_name)
                    .into_iter()
                    .map(|view| {
                        let mut m = Map::new();
                        m.insert("vnum".into(), Dynamic::from(view.vnum));
                        m.insert("name".into(), Dynamic::from(view.name));
                        m.insert("summary".into(), Dynamic::from(view.summary));
                        let lines: Array = view
                            .objective_lines
                            .into_iter()
                            .map(Dynamic::from)
                            .collect();
                        m.insert("lines".into(), Dynamic::from(lines));
                        Dynamic::from(m)
                    })
                    .collect()
            },
        );
    }

    // quest_show_detail(player_name, quest_vnum) -> Map (empty when no match).
    // Includes status (active|completed|none), progress lines, and reward
    // summary lines.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "quest_show_detail",
            move |player_name: String, quest_vnum: String| -> Map {
                let mut m = Map::new();
                let quest = match crate::quest::find_quest(&cloned_db, &quest_vnum) {
                    Some(q) => q,
                    None => return m,
                };
                let ch = match cloned_db.get_character_data(&player_name.to_lowercase()) {
                    Ok(Some(c)) => c,
                    _ => return m,
                };
                let status = if ch.completed_quests.contains(&quest.vnum) {
                    "completed"
                } else if ch.active_quests.contains_key(&quest.vnum) {
                    "active"
                } else {
                    "none"
                };
                m.insert("vnum".into(), Dynamic::from(quest.vnum.clone()));
                m.insert("name".into(), Dynamic::from(quest.name.clone()));
                m.insert("summary".into(), Dynamic::from(quest.summary.clone()));
                m.insert("description".into(), Dynamic::from(quest.description.clone()));
                m.insert("status".into(), Dynamic::from(status.to_string()));
                m.insert("repeatable".into(), Dynamic::from(quest.repeatable));

                // Progress lines (only meaningful while active).
                let mut progress: Array = Array::new();
                if status == "active" {
                    if let Some(p) = ch.active_quests.get(&quest.vnum) {
                        for line in render_progress_lines(&cloned_db, &ch, p, &quest) {
                            progress.push(Dynamic::from(line));
                        }
                    }
                }
                m.insert("progress".into(), Dynamic::from(progress));

                // Reward summaries.
                let rewards: Array = quest
                    .rewards
                    .iter()
                    .map(|r| Dynamic::from(format_reward(r)))
                    .collect();
                m.insert("rewards".into(), Dynamic::from(rewards));
                m
            },
        );
    }

    // resolve_quest_keyword(needle) -> String (vnum or "")
    {
        let cloned_db = db.clone();
        engine.register_fn("resolve_quest_keyword", move |needle: String| -> String {
            crate::quest::find_quest(&cloned_db, &needle)
                .map(|q| q.vnum)
                .unwrap_or_default()
        });
    }

    // describe_quest_objectives(vnum) -> Array<String>
    // Builder-side rendering used by `quedit show`.
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "describe_quest_objectives",
            move |vnum: String| -> Array {
                let q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return Array::new(),
                };
                q.objectives
                    .iter()
                    .map(|o| Dynamic::from(format_objective_summary(o)))
                    .collect()
            },
        );
    }

    // describe_quest_rewards(vnum) -> Array<String>
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "describe_quest_rewards",
            move |vnum: String| -> Array {
                let q = match cloned_db.get_quest_data(&vnum) {
                    Ok(Some(q)) => q,
                    _ => return Array::new(),
                };
                q.rewards
                    .iter()
                    .map(|r| Dynamic::from(format_reward(r)))
                    .collect()
            },
        );
    }
}

fn push_objective(db: &Db, vnum: &str, obj: QuestObjective) -> String {
    let mut q = match db.get_quest_data(vnum) {
        Ok(Some(q)) => q,
        _ => return format!("no such quest `{}`", vnum),
    };
    q.objectives.push(obj);
    if let Err(e) = db.save_quest_data(&q) {
        return format!("save error: {}", e);
    }
    String::new()
}

fn push_reward(db: &Db, vnum: &str, reward: QuestReward) -> String {
    let mut q = match db.get_quest_data(vnum) {
        Ok(Some(q)) => q,
        _ => return format!("no such quest `{}`", vnum),
    };
    q.rewards.push(reward);
    if let Err(e) = db.save_quest_data(&q) {
        return format!("save error: {}", e);
    }
    String::new()
}

fn format_objective_summary(obj: &QuestObjective) -> String {
    match obj {
        QuestObjective::KillMob { vnum, count } => format!("Kill {} (x{})", vnum, count),
        QuestObjective::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } => match return_to_mob_vnum {
            Some(mv) => format!("Bring {} (x{}) → {}", vnum, qty, mv),
            None => format!("Bring {} (x{})", vnum, qty),
        },
        QuestObjective::VisitRoom { vnum } => format!("Visit room {}", vnum),
        QuestObjective::DgFlag { var, value } => format!("Set flag {}={}", var, value),
    }
}

fn format_reward(reward: &QuestReward) -> String {
    match reward {
        QuestReward::Gold { amount } => format!("{} gold", amount),
        QuestReward::Item { vnum, qty } => format!("item {} (x{})", vnum, qty),
        QuestReward::SkillXp { skill, amount } => format!("{} {} xp", amount, skill),
        QuestReward::Achievement { key } => format!("achievement {}", key),
        QuestReward::LearnRecipe { recipe_id } => format!("recipe {}", recipe_id),
        QuestReward::EmbraceClan { clan } => format!("embrace clan {}", clan),
    }
}

fn render_progress_lines(
    db: &Db,
    ch: &crate::types::CharacterData,
    progress: &ActiveQuest,
    quest: &QuestData,
) -> Vec<String> {
    let mut out = Vec::new();
    for obj in &quest.objectives {
        out.push(render_one_progress_line(db, ch, progress, obj));
    }
    out
}

fn render_one_progress_line(
    db: &Db,
    ch: &crate::types::CharacterData,
    progress: &ActiveQuest,
    obj: &QuestObjective,
) -> String {
    match obj {
        QuestObjective::KillMob { vnum, count } => {
            let cur = progress.kill_progress.get(vnum).copied().unwrap_or(0);
            let label = db
                .get_mobile_by_vnum(vnum)
                .ok()
                .flatten()
                .map(|m| m.short_desc)
                .unwrap_or_else(|| format!("mob {}", vnum));
            format!("Slay {}: {}/{}", label, cur, count)
        }
        QuestObjective::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } => {
            let label = db
                .get_item_by_vnum(vnum)
                .ok()
                .flatten()
                .map(|i| i.short_desc)
                .unwrap_or_else(|| format!("item {}", vnum));
            let cur = if return_to_mob_vnum.is_some() {
                progress.item_progress.get(vnum).copied().unwrap_or(0)
            } else {
                count_inventory_vnum_for(db, &ch.name, vnum).min(*qty)
            };
            let suffix = match return_to_mob_vnum {
                Some(mv) => {
                    let mname = db
                        .get_mobile_by_vnum(mv)
                        .ok()
                        .flatten()
                        .map(|m| m.short_desc)
                        .unwrap_or_else(|| format!("mob {}", mv));
                    format!(" (deliver to {})", mname)
                }
                None => String::new(),
            };
            format!("Collect {}: {}/{}{}", label, cur, qty, suffix)
        }
        QuestObjective::VisitRoom { vnum } => {
            let visited = progress.rooms_visited.contains(vnum);
            format!("Visit room {}: {}", vnum, if visited { "✓" } else { "·" })
        }
        QuestObjective::DgFlag { var, value } => {
            let set = progress.flags_set.contains(var);
            format!("Set flag {}={}: {}", var, value, if set { "✓" } else { "·" })
        }
    }
}

fn count_inventory_vnum_for(db: &Db, char_name: &str, vnum: &str) -> i32 {
    let mut n = 0;
    if let Ok(items) = db.list_all_items() {
        for item in items {
            if item.is_prototype || item.vnum.as_deref() != Some(vnum) {
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

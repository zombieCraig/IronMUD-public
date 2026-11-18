// src/script/spellcraft.rs
// Thin Rhai layer over `crate::spellcraft`. Rules live there.

use crate::db::Db;
use crate::spellcraft::codex::{self as codex_ops, status_str};
use crate::spellcraft::{self, LearnVia, Settings, power};
use crate::types::{CodexEntry, LineageKind, SpellOrigin};
use crate::{SharedConnections, SharedState};
use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;

fn origin_str(o: SpellOrigin) -> &'static str {
    match o {
        SpellOrigin::Ancient => "ancient",
        SpellOrigin::Authored => "authored",
    }
}

/// Everything a script may show about a codex spell's history.
fn meta_map(entry: &CodexEntry) -> Map {
    let m = &entry.meta;
    let mut out = Map::new();
    out.insert("id".into(), Dynamic::from(entry.def.id.clone()));
    out.insert("name".into(), Dynamic::from(entry.def.name.clone()));
    out.insert("spell_type".into(), Dynamic::from(entry.def.spell_type.clone()));
    out.insert("origin".into(), Dynamic::from(origin_str(m.origin).to_string()));
    out.insert("author".into(), Dynamic::from(m.author.clone().unwrap_or_default()));
    out.insert("god_vnum".into(), Dynamic::from(m.god_vnum.clone()));
    out.insert("status".into(), Dynamic::from(status_str(m.status).to_string()));
    out.insert("sealed_day".into(), Dynamic::from(m.sealed_day));
    out.insert("author_disgraced".into(), Dynamic::from(m.author_disgraced));
    out.insert("vetoed".into(), Dynamic::from(m.vetoed));
    out.insert("version".into(), Dynamic::from(m.version as i64));
    out.insert("tier".into(), Dynamic::from(power::spell_tier(&entry.def) as i64));
    let lineage: Array = m
        .lineage
        .iter()
        .map(|l| {
            let mut e = Map::new();
            let kind = match l.kind {
                LineageKind::Authored => "authored",
                LineageKind::Restored => "restored",
            };
            e.insert("kind".into(), Dynamic::from(kind.to_string()));
            e.insert("who".into(), Dynamic::from(l.who.clone()));
            e.insert("day".into(), Dynamic::from(l.day));
            Dynamic::from(e)
        })
        .collect();
    out.insert("lineage".into(), Dynamic::from(lineage));
    let witnesses: Array = m.witnesses.iter().cloned().map(Dynamic::from).collect();
    out.insert("witnesses".into(), Dynamic::from(witnesses));
    out
}

fn ok_or_message(r: Result<CodexEntry, String>) -> String {
    match r {
        Ok(_) => "ok".to_string(),
        Err(e) => e,
    }
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // spellcraft_enabled() -> bool: the `spellcraft.enabled` master switch.
    let cloned_db = db.clone();
    engine.register_fn("spellcraft_enabled", move || -> bool {
        Settings::load(&cloned_db).enabled
    });

    // spellcraft_setting_keys() -> Array of the `spellcraft.*` keys
    // `admin config` may change.
    engine.register_fn("spellcraft_setting_keys", || -> Array {
        spellcraft::setting_keys()
            .map(|k| Dynamic::from(k.to_string()))
            .collect()
    });

    // spellcraft_learn(char_name, spell_id, via) -> String
    //   via: "scroll" | "temple" | "evolution" | "admin"
    //   result: "learned" | "already_known" | "unknown_spell" | "no_character"
    //           | "not_in_canon" | "needs_confirm" | "bad_via"
    // Every script learn path goes through this. "needs_confirm": another
    // god's spell; show `heresy_learn_warning` and ask for confirmation,
    // then call `spellcraft_learn_confirmed` within a minute.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "spellcraft_learn",
        move |char_name: String, spell_id: String, via: String| -> String {
            let Some(via) = LearnVia::parse(&via) else {
                return "bad_via".to_string();
            };
            spellcraft::learn(&cloned_db, &cloned_conns, &cloned_state, &char_name, &spell_id, via)
                .as_str()
                .to_string()
        },
    );

    // spellcraft_learn_confirmed(char_name, spell_id, via) -> String: the
    // same, completing an open heresy confirmation.
    let (d, c, st) = (db.clone(), connections.clone(), state.clone());
    engine.register_fn(
        "spellcraft_learn_confirmed",
        move |char_name: String, spell_id: String, via: String| -> String {
            let Some(via) = LearnVia::parse(&via) else {
                return "bad_via".to_string();
            };
            spellcraft::learn_with(
                &d,
                &c,
                &st,
                &char_name,
                &spell_id,
                via,
                true,
                spellcraft::ritual::now(),
                &mut spellcraft::ritual::ThreadDice,
            )
            .as_str()
            .to_string()
        },
    );
    // heresy_learn_warning(char_name, spell_id) -> String ("" if not heresy).
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn(
        "heresy_learn_warning",
        move |char_name: String, spell_id: String| -> String {
            spellcraft::heresy::learn_warning(&d, &c, &char_name, &spell_id)
        },
    );
    // codex_heresy(char_name, spell_id) -> String: the cast-tail heresy
    // (backlash, grudges, the gods' rolls). Lines for the caster, or "".
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("codex_heresy", move |char_name: String, spell_id: String| -> String {
        spellcraft::heresy::on_foreign_cast(&d, &c, &char_name, &spell_id, &mut spellcraft::ritual::ThreadDice)
    });
    // worship_renounce(char_name, confirm) -> String.
    let (d, c, st) = (db.clone(), connections.clone(), state.clone());
    engine.register_fn("worship_renounce", move |char_name: String, confirm: bool| -> String {
        spellcraft::heresy::renounce(&d, &c, &st, &char_name, confirm, spellcraft::ritual::now())
    });
    // divine_grudges(char_name) -> Array of display lines, strongest first.
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("divine_grudges", move |char_name: String| -> Array {
        crate::script::achievements::apply_to_character(&d, &c, &char_name, |_| false)
            .map(|ch| spellcraft::heresy::grudge_lines(&d, &ch))
            .unwrap_or_default()
            .into_iter()
            .map(Dynamic::from)
            .collect()
    });
    // clear_divine_grudge(char_name, god_vnum) -> bool (admin).
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn(
        "clear_divine_grudge",
        move |char_name: String, god_vnum: String| -> bool {
            spellcraft::heresy::clear_grudge(&d, &c, &char_name, &god_vnum)
        },
    );

    // get_codex_meta(spell_id) -> Map; empty when the spell is not a codex
    // spell. Keys: id, name, spell_type, origin, author, god_vnum, status,
    // sealed_day, author_disgraced, vetoed, version, tier, lineage (array of
    // {kind, who, day}), witnesses (array of names).
    let cloned_db = db.clone();
    engine.register_fn("get_codex_meta", move |spell_id: String| -> Map {
        match cloned_db.get_codex(&spell_id) {
            Ok(Some(entry)) => meta_map(&entry),
            _ => Map::new(),
        }
    });

    // codex_list(include_vetoed) -> Array of the same maps as get_codex_meta,
    // sorted by name.
    let cloned_db = db.clone();
    engine.register_fn("codex_list", move |include_vetoed: bool| -> Array {
        let mut entries = cloned_db.list_codex().unwrap_or_default();
        entries.retain(|e| include_vetoed || !e.meta.vetoed);
        entries.sort_by_key(|e| e.def.name.to_lowercase());
        entries.iter().map(|e| Dynamic::from(meta_map(e))).collect()
    });

    // spellcraft_scribe(name, spell_query) -> String: `craft scroll`.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn("spellcraft_scribe", move |name: String, query: String| -> String {
        crate::spellcraft::scribe::scribe(&cloned_db, &cloned_conns, &cloned_state, &name, &query)
    });

    // canon_influences() -> Array of #{god_vnum, influence, held} for every
    // god with a canon, most influential first. Influence is the canon's
    // summed devotion; `held` is how many spells it holds.
    let cloned_db = db.clone();
    engine.register_fn("canon_influences", move || -> Array {
        let mut canons = cloned_db.list_canons().unwrap_or_default();
        canons.sort_by(|a, b| b.influence.cmp(&a.influence).then(a.god_vnum.cmp(&b.god_vnum)));
        canons
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("god_vnum".into(), Dynamic::from(c.god_vnum.clone()));
                m.insert("influence".into(), Dynamic::from(c.influence as i64));
                m.insert("held".into(), Dynamic::from(c.slots.len() as i64));
                Dynamic::from(m)
            })
            .collect()
    });

    // spell_power_tier(spell_id) -> i64: 1-5, or 0 for unknown spells and
    // types outside the power scale.
    let cloned_state = state.clone();
    engine.register_fn("spell_power_tier", move |spell_id: String| -> i64 {
        let world = cloned_state.lock();
        world
            .spell_definitions
            .get(&spell_id)
            .map(|d| power::spell_tier(d) as i64)
            .unwrap_or(0)
    });

    // find_spell_id(query) -> String: exact id, exact name, then a unique
    // name/id prefix, over every defined spell. "" when none or ambiguous.
    let cloned_state = state.clone();
    engine.register_fn("find_spell_id", move |query: String| -> String {
        let q = query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if q.is_empty() {
            return String::new();
        }
        let q_id = q.replace(' ', "_");
        let world = cloned_state.lock();
        let spells = &world.spell_definitions;
        if spells.contains_key(&q_id) {
            return q_id;
        }
        if let Some((id, _)) = spells.iter().find(|(_, d)| d.name.to_lowercase() == q) {
            return id.clone();
        }
        let mut hits: Vec<&String> = spells
            .iter()
            .filter(|(id, d)| d.name.to_lowercase().starts_with(&q) || id.starts_with(&q_id))
            .map(|(id, _)| id)
            .collect();
        hits.sort();
        hits.dedup();
        if hits.len() == 1 {
            hits[0].clone()
        } else {
            String::new()
        }
    });

    // ----- admin codex moderation (admin.rhai gates on is_admin) -----

    // codex_set_status(spell_id, "canon"|"faded"|"lost") -> "ok" or message
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("codex_set_status", move |spell_id: String, status: String| -> String {
        let Some(status) = codex_ops::parse_status(&status) else {
            return "Status must be canon, faded or lost.".to_string();
        };
        ok_or_message(codex_ops::set_status(&cloned_db, &cloned_state, &spell_id, status))
    });

    // codex_set_veto(spell_id, vetoed) -> "ok" or message
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("codex_set_veto", move |spell_id: String, vetoed: bool| -> String {
        ok_or_message(codex_ops::set_veto(&cloned_db, &cloned_state, &spell_id, vetoed))
    });

    // codex_rename(spell_id, name) -> "ok" or message
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("codex_rename", move |spell_id: String, name: String| -> String {
        ok_or_message(codex_ops::rename(&cloned_db, &cloned_state, &spell_id, &name))
    });

    // codex_seed() -> String summary. Re-reads the Ancient seed file and
    // adds entries whose ids are new; never overwrites.
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("codex_seed", move || -> String {
        // Seed into a copy of the spell map (validation needs it), then
        // mirror the additions; the World lock is not held across the DB work.
        let mut spells = cloned_state.lock().spell_definitions.clone();
        let report = codex_ops::seed_ancient(&cloned_db, &mut spells);
        for id in &report.added {
            if let Ok(Some(entry)) = cloned_db.get_codex(id) {
                spellcraft::mirror_into_world(&cloned_state, &entry);
            }
        }
        report.summary()
    });

    register_deity_and_gate(engine, db.clone(), connections.clone());
    register_price(engine, db.clone(), connections.clone());
    register_ritual(engine, db.clone(), connections.clone(), state.clone());
    register_canon(engine, db.clone(), connections.clone(), state.clone());
    register_petition(engine, db, connections, state);
}

/// The character as the game currently sees them: the session copy when
/// online (authoritative), else the DB row. Connections is released before
/// returning.
fn current_character(db: &Db, connections: &SharedConnections, name: &str) -> Option<crate::CharacterData> {
    {
        let conns = connections.lock();
        if let Some(ch) = conns
            .values()
            .find_map(|s| s.character.as_ref().filter(|c| c.name.eq_ignore_ascii_case(name)))
        {
            return Some(ch.clone());
        }
    }
    db.get_character_data(&name.to_lowercase()).ok().flatten()
}

fn str_array(v: &[String]) -> Array {
    v.iter().cloned().map(Dynamic::from).collect()
}

fn opt_int(v: Option<i64>) -> Dynamic {
    match v {
        Some(n) => Dynamic::from(n),
        None => Dynamic::UNIT,
    }
}

fn register_deity_and_gate(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    use crate::spellcraft::{deity, gate};
    use crate::types::DeitySpellcraft;

    // deity_spellcraft_get(mobile_id) -> Map, or () when the mobile is not a
    // deity or grants no spells. Override keys are () when unset.
    let d = db.clone();
    engine.register_fn("deity_spellcraft_get", move |mobile_id: String| -> Dynamic {
        let Ok(id) = uuid::Uuid::parse_str(&mobile_id) else {
            return Dynamic::UNIT;
        };
        let Some(c) = d
            .get_mobile_data(&id)
            .ok()
            .flatten()
            .and_then(|m| m.deity)
            .and_then(|dc| dc.spellcraft)
        else {
            return Dynamic::UNIT;
        };
        let mut m = Map::new();
        m.insert("allowed_types".into(), Dynamic::from(str_array(&c.allowed_types)));
        m.insert("allowed_elements".into(), Dynamic::from(str_array(&c.allowed_elements)));
        m.insert("allowed_effects".into(), Dynamic::from(str_array(&c.allowed_effects)));
        m.insert("allowed_targets".into(), Dynamic::from(str_array(&c.allowed_targets)));
        m.insert("canon_cap".into(), Dynamic::from(c.canon_cap as i64));
        m.insert("petition_cooldown_days".into(), opt_int(c.petition_cooldown_days));
        m.insert("min_favor".into(), opt_int(c.min_favor));
        m.insert("min_power_tier".into(), opt_int(c.min_power_tier));
        m.insert("favor_cost".into(), Dynamic::from(c.favor_cost));
        m.insert(
            "unlock_quest_id".into(),
            Dynamic::from(c.unlock_quest_id.unwrap_or_default()),
        );
        m.insert("priest_teaches".into(), Dynamic::from(c.priest_teaches));
        m.insert("price_style".into(), Dynamic::from(c.price_style.as_str().to_string()));
        let trials: rhai::Array = c
            .trial_rules
            .iter()
            .map(|r| Dynamic::from(deity::describe_trial(r)))
            .collect();
        m.insert("trials".into(), Dynamic::from(trials));
        let marks: rhai::Array = c
            .mark_pool
            .iter()
            .map(|t| Dynamic::from(deity::describe_mark(t)))
            .collect();
        m.insert("marks".into(), Dynamic::from(marks));
        Dynamic::from(m)
    });

    // deity_spellcraft_enable(mobile_id, on) -> "ok" or message. Enabling
    // keeps an existing config; disabling removes it.
    let d = db.clone();
    engine.register_fn(
        "deity_spellcraft_enable",
        move |mobile_id: String, on: bool| -> String {
            let Ok(id) = uuid::Uuid::parse_str(&mobile_id) else {
                return "Invalid mobile.".to_string();
            };
            let mut was_deity = false;
            let res = d.update_mobile(&id, |m| {
                was_deity = m.deity.is_some();
                if let Some(dc) = m.deity.as_mut() {
                    if on {
                        dc.spellcraft.get_or_insert_with(DeitySpellcraft::default);
                    } else {
                        dc.spellcraft = None;
                    }
                }
            });
            match res {
                Ok(Some(_)) if was_deity => "ok".to_string(),
                Ok(Some(_)) => "This mobile is not a deity.".to_string(),
                _ => "Mobile not found.".to_string(),
            }
        },
    );

    // deity_spellcraft_edit(mobile_id, field, op, value) -> "ok" or message.
    // See `spellcraft::deity::edit` for fields.
    let d = db.clone();
    engine.register_fn(
        "deity_spellcraft_edit",
        move |mobile_id: String, field: String, op: String, value: String| -> String {
            let Ok(id) = uuid::Uuid::parse_str(&mobile_id) else {
                return "Invalid mobile.".to_string();
            };
            // The CAS closure may rerun; it records only its latest outcome.
            let mut outcome: Result<(), String> = Err("Mobile not found.".to_string());
            let res = d.update_mobile(&id, |m| {
                outcome = match m.deity.as_mut().and_then(|dc| dc.spellcraft.as_mut()) {
                    Some(cfg) => {
                        let mut next = cfg.clone();
                        let r = deity::edit(&mut next, &field, &op, &value);
                        if r.is_ok() {
                            *cfg = next;
                        }
                        r
                    }
                    None => Err("Spellcraft is off for this god. Use 'deity spellcraft enable' first.".to_string()),
                };
            });
            match (res, outcome) {
                (Ok(Some(_)), Ok(())) => "ok".to_string(),
                (Ok(Some(_)), Err(e)) => e,
                _ => "Mobile not found.".to_string(),
            }
        },
    );

    // petition_checklist(char_name) -> Map
    //   enabled, worships (bool), god_vnum, god_name, god_grants, can_petition,
    //   requirements: [{key, label, met, detail}]
    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("petition_checklist", move |char_name: String| -> Map {
        let settings = Settings::load(&d);
        let mut out = Map::new();
        out.insert("enabled".into(), Dynamic::from(settings.enabled));
        let Some(ch) = current_character(&d, &co, &char_name) else {
            out.insert("worships".into(), Dynamic::from(false));
            return out;
        };
        let Some(god_vnum) = ch.worship.as_ref().map(|w| w.god_vnum.clone()) else {
            out.insert("worships".into(), Dynamic::from(false));
            return out;
        };
        let status = gate::petition_status(&d, &ch, &god_vnum, &settings, gate::Clock::now(&d));
        out.insert("worships".into(), Dynamic::from(true));
        out.insert("god_vnum".into(), Dynamic::from(status.god_vnum.clone()));
        out.insert("god_name".into(), Dynamic::from(status.god_name.clone()));
        out.insert("god_grants".into(), Dynamic::from(status.god_grants));
        out.insert("can_petition".into(), Dynamic::from(status.can_petition()));
        let reqs: Array = status
            .requirements
            .iter()
            .map(|r| {
                let mut m = Map::new();
                m.insert("key".into(), Dynamic::from(r.key.to_string()));
                m.insert("label".into(), Dynamic::from(r.label.clone()));
                m.insert("met".into(), Dynamic::from(r.met));
                m.insert("detail".into(), Dynamic::from(r.detail.clone()));
                Dynamic::from(m)
            })
            .collect();
        out.insert("requirements".into(), Dynamic::from(reqs));
        out
    });
}

/// The petition drafting mode (`spellcraft::petition`).
fn register_price(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    use crate::script::achievements::apply_to_character;
    use crate::spellcraft::price;

    // divine_pledge_status(name) -> String: the `status` block ("" if none).
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("divine_pledge_status", move |name: String| -> String {
        apply_to_character(&d, &c, &name, |_| false)
            .map(|ch| price::status_lines(&d, &ch))
            .unwrap_or_default()
    });

    // get_character_marks(name) -> Array of display strings, for examine.
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("get_character_marks", move |name: String| -> Array {
        apply_to_character(&d, &c, &name, |_| false)
            .map(|ch| price::mark_lines(&ch).into_iter().map(Dynamic::from).collect())
            .unwrap_or_default()
    });

    // admin_pledge(args) / admin_mark(args) -> String: admin subcommands;
    // `args` is everything after the subcommand word.
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("admin_pledge", move |args: String| -> String {
        let words: Vec<&str> = args.split_whitespace().collect();
        price::admin_pledge(&d, &c, &words)
    });
    let (d, c) = (db, connections);
    engine.register_fn("admin_mark", move |args: String| -> String {
        let words: Vec<&str> = args.split_whitespace().collect();
        price::admin_mark(&d, &c, &words)
    });
}

fn register_ritual(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    use crate::spellcraft::ritual::{self, ThreadDice};

    /// The connection's character name.
    fn who(conns: &SharedConnections, id: &str) -> Option<String> {
        let id = uuid::Uuid::parse_str(id).ok()?;
        conns.lock().get(&id)?.character.as_ref().map(|c| c.name.clone())
    }

    // ritual_begin / ritual_push / ritual_seal (connection_id) -> String.
    let (d, c, st) = (db.clone(), connections.clone(), state.clone());
    engine.register_fn("ritual_begin", move |connection_id: String| -> String {
        match who(&c, &connection_id) {
            Some(n) => ritual::begin(&d, &c, &st, &n, ritual::now(), &mut ThreadDice),
            None => String::new(),
        }
    });
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("ritual_push", move |connection_id: String| -> String {
        match who(&c, &connection_id) {
            Some(n) => ritual::push(&d, &c, &n, ritual::now(), &mut ThreadDice),
            None => String::new(),
        }
    });
    let (d, c, st) = (db.clone(), connections.clone(), state.clone());
    engine.register_fn("ritual_seal", move |connection_id: String| -> String {
        match who(&c, &connection_id) {
            Some(n) => ritual::seal(&d, &c, &st, &n),
            None => String::new(),
        }
    });
    // ritual_aid(connection_id, target) -> String.
    let (d, c, st) = (db.clone(), connections.clone(), state);
    engine.register_fn("ritual_aid", move |connection_id: String, target: String| -> String {
        match who(&c, &connection_id) {
            Some(n) => ritual::aid(&d, &c, &st, &n, &target, &mut ThreadDice),
            None => String::new(),
        }
    });
    // ritual_in_view(connection_id) -> bool: a rite is under way for this
    // player or in their room (gates the push/seal/aid help lines).
    let c = connections;
    engine.register_fn("ritual_in_view", move |connection_id: String| -> bool {
        uuid::Uuid::parse_str(&connection_id).is_ok_and(|id| ritual::in_view(&c, id))
    });
    let _ = db;
}

fn register_canon(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    use crate::spellcraft::canon;

    // codex_cast_info(char_name, spell_id, spell_name) -> Map, always with
    // every key: codex, true_form, seen_as, flavor_caster, flavor_room,
    // flavor_target ("" when unset).
    let d = db.clone();
    engine.register_fn(
        "codex_cast_info",
        move |char_name: String, spell_id: String, spell_name: String| -> rhai::Map {
            let i = canon::cast_info(&d, &char_name, &spell_id, &spell_name);
            let mut m = rhai::Map::new();
            m.insert("codex".into(), i.codex.into());
            m.insert("true_form".into(), i.true_form.into());
            m.insert("seen_as".into(), i.seen_as.into());
            m.insert("flavor_caster".into(), i.caster.into());
            m.insert("flavor_room".into(), i.room.into());
            m.insert("flavor_target".into(), i.target.into());
            m
        },
    );
    // codex_note_cast(char_name, spell_id): count a faithful cast toward
    // the spell's devotion. No-op for ordinary spells.
    let (d, c, st) = (db.clone(), connections.clone(), state);
    engine.register_fn("codex_note_cast", move |char_name: String, spell_id: String| {
        canon::note_cast(&d, &c, &st, &char_name, &spell_id);
    });
    // pray_restore(connection_id, spell) -> String.
    let (d, c) = (db, connections);
    engine.register_fn("pray_restore", move |connection_id: String, spell: String| -> String {
        let name = uuid::Uuid::parse_str(&connection_id)
            .ok()
            .and_then(|id| c.lock().get(&id)?.character.as_ref().map(|ch| ch.name.clone()));
        match name {
            Some(n) => canon::restore(&d, &c, &n, &spell),
            None => String::new(),
        }
    });
}

fn register_petition(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    use crate::spellcraft::petition;

    fn conn(id: &str) -> Option<uuid::Uuid> {
        uuid::Uuid::parse_str(id).ok()
    }

    // petition_begin(connection_id) -> String: opens the drafting mode and
    // returns its first prompt; "" when the character may not petition.
    let (d, c) = (db.clone(), connections.clone());
    engine.register_fn("petition_begin", move |connection_id: String| -> String {
        conn(&connection_id)
            .and_then(|id| petition::begin(&d, &c, id))
            .unwrap_or_default()
    });

    // petition_input(connection_id, line) -> #{ text, offer, name,
    // spell_type, power, tier }. `offer` true = fire the god's OnPray
    // (action "petition") trigger, then call petition_offer unless vetoed.
    let (d, c, st) = (db.clone(), connections.clone(), state.clone());
    engine.register_fn("petition_input", move |connection_id: String, line: String| -> Map {
        let r = match conn(&connection_id) {
            Some(id) => petition::input(&d, &c, &st, id, &line),
            None => petition::Reply::default(),
        };
        let mut m = Map::new();
        m.insert("text".into(), Dynamic::from(r.text));
        m.insert("offer".into(), Dynamic::from(r.offer.is_some()));
        let p = r.offer;
        m.insert(
            "name".into(),
            Dynamic::from(p.as_ref().map(|p| p.name.clone()).unwrap_or_default()),
        );
        m.insert(
            "spell_type".into(),
            Dynamic::from(p.as_ref().map(|p| p.spell_type.clone()).unwrap_or_default()),
        );
        m.insert(
            "power".into(),
            Dynamic::from(p.as_ref().map(|p| p.power as i64).unwrap_or(0)),
        );
        m.insert(
            "tier".into(),
            Dynamic::from(p.as_ref().map(|p| p.tier as i64).unwrap_or(0)),
        );
        m
    });

    // petition_offer(connection_id) -> String: lay the reviewed draft before
    // the god and report the outcome.
    let (d, c, st) = (db.clone(), connections.clone(), state);
    engine.register_fn("petition_offer", move |connection_id: String| -> String {
        match conn(&connection_id) {
            Some(id) => petition::offer(&d, &c, &st, id),
            None => String::new(),
        }
    });

    // petition_pending(char_name) -> String: a line about an offered
    // petition, "" when there is none.
    let (d, c) = (db, connections);
    engine.register_fn("petition_pending", move |char_name: String| -> String {
        petition::pending_status(&d, &c, &char_name).unwrap_or_default()
    });
}

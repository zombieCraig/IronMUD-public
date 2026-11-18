//! Divine spellcraft, end to end through the scripts.

use ironmud::db::Db;
use ironmud::types::{CanonStatus, CodexEntry, CodexMeta};
use ironmud::{ItemData, PlayerSession, World};
use parking_lot::Mutex;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// A world with one online player, ready to run command scripts as them.
struct Harness {
    db: Db,
    state: ironmud::SharedState,
    connections: ironmud::SharedConnections,
    conn_id: Uuid,
    out_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    _temp: tempfile::TempDir,
}

impl Harness {
    fn new(name: &str, is_admin: bool) -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let state = World::minimal_shared(db.clone(), connections.clone());
        state.lock().command_metadata = ironmud::load_command_metadata().expect("commands.json");

        let ch: ironmud::CharacterData = serde_json::from_value(json!({
            "name": name, "password_hash": "", "current_room_id": Uuid::nil(), "is_admin": is_admin,
            "is_builder": is_admin,
            "skills": { "magic": { "level": 3, "experience": 0 } },
        }))
        .unwrap();
        db.save_character_data(ch.clone()).unwrap();

        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
        let conn_id = Uuid::new_v4();
        let mut session = PlayerSession::new_for_test(out_tx, in_tx);
        session.character = Some(ch);
        connections.lock().insert(conn_id, session);
        std::mem::forget(_in_rx);
        Harness {
            db,
            state,
            connections,
            conn_id,
            out_rx,
            _temp: temp,
        }
    }

    fn run(&mut self, script: &str, args: &str) -> String {
        let mut engine = rhai::Engine::new();
        engine.set_max_expr_depths(128, 128);
        let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
        resolver.set_base_path("scripts/lib");
        engine.set_module_resolver(resolver);
        ironmud::script::register_rhai_functions(
            &mut engine,
            Arc::new(self.db.clone()),
            self.connections.clone(),
            self.state.clone(),
        );
        let ast = engine
            .compile_file(format!("scripts/commands/{script}.rhai").into())
            .expect("compile");
        let mut scope = rhai::Scope::new();
        let r: Result<(), _> = engine.call_fn(
            &mut scope,
            &ast,
            "run_command",
            (args.to_string(), self.conn_id.to_string()),
        );
        r.unwrap_or_else(|e| panic!("{script} {args}: {e}"));
        let mut out = String::new();
        while let Ok(chunk) = self.out_rx.try_recv() {
            out.push_str(&chunk);
        }
        out
    }

    fn add_codex(&self, id: &str, name: &str, status: CanonStatus) {
        let entry = CodexEntry {
            def: serde_json::from_value(json!({
                "id": id, "name": name, "description": "A lance of living flame.",
                "spell_type": "damage", "mana_cost": 40, "damage_base": 60, "target_type": "enemy",
            }))
            .unwrap(),
            meta: CodexMeta {
                god_vnum: "god:sol".into(),
                author: Some("Mira".into()),
                status,
                ..CodexMeta::default()
            },
        };
        assert!(ironmud::spellcraft::insert_codex_spell(&self.db, &self.state, &entry).unwrap());
    }

    fn give_scroll(&self, owner: &str, spell_id: &str) -> Uuid {
        let mut scroll = ItemData::new(
            "scroll ember".to_string(),
            "a scorched scroll".to_string(),
            "A scroll, warm to the touch.".to_string(),
        );
        scroll.teaches_spell = Some(spell_id.to_string());
        let id = scroll.id;
        self.db.save_item_data(scroll).unwrap();
        self.db.move_item_to_inventory(&id, owner).unwrap();
        id
    }

    /// Change the online player's character (session) and save it.
    fn edit_live(&self, f: impl FnOnce(&mut ironmud::CharacterData)) {
        let mut conns = self.connections.lock();
        let ch = conns.get_mut(&self.conn_id).unwrap().character.as_mut().unwrap();
        f(ch);
        self.db.save_character_data(ch.clone()).unwrap();
    }

    /// A temple room, with the player standing in it.
    fn enter_temple(&self) {
        let room: ironmud::RoomData = serde_json::from_value(json!({
            "id": Uuid::new_v4(), "title": "Temple of Sol", "description": "Gold light.",
            "exits": {}, "flags": { "temple": true },
        }))
        .unwrap();
        let id = room.id;
        self.db.save_room_data(room).unwrap();
        self.edit_live(|c| c.current_room_id = id);
    }

    /// A worshipable god prototype; `craft` = it grants spells.
    fn god(&self, vnum: &str, craft: bool) {
        let mut m = ironmud::MobileData::new("Sol".to_string());
        m.vnum = vnum.to_string();
        m.is_prototype = true;
        m.keywords = vec!["sol".into()];
        m.deity = Some(ironmud::types::DeityConfig {
            spellcraft: craft.then(ironmud::types::DeitySpellcraft::default),
            ..Default::default()
        });
        self.db.save_mobile_data(m).unwrap();
    }

    fn live(&self) -> ironmud::CharacterData {
        self.connections
            .lock()
            .get(&self.conn_id)
            .unwrap()
            .character
            .clone()
            .unwrap()
    }
}

#[test]
fn reading_a_scroll_of_a_codex_spell_teaches_it_and_survives_a_flush() {
    let mut h = Harness::new("Pupil", false);
    h.add_codex("cx_ember_lance", "Ember Lance", CanonStatus::Canon);
    let scroll_id = h.give_scroll("Pupil", "cx_ember_lance");

    let out = h.run("read", "scroll");
    assert!(
        out.contains("You have learned: Ember Lance"),
        "unexpected output:\n{out}"
    );
    assert!(h.db.get_item_data(&scroll_id).unwrap().is_none(), "scroll consumed");

    // The regen tick flushes the session over the DB; the spell must stay.
    let live = h.live();
    assert!(live.learned_spells.contains(&"cx_ember_lance".to_string()));
    h.db.save_character_data(live).unwrap();
    let stored = h.db.get_character_data("pupil").unwrap().unwrap();
    assert!(stored.learned_spells.contains(&"cx_ember_lance".to_string()));
}

/// S2's definition of done: an Ancient spell from the seed format, its scroll
/// read by a player, then visible in `spells codex` with provenance.
#[test]
fn an_ancient_spell_is_seeded_learned_and_browsable() {
    let mut h = Harness::new("Seeker", false);
    let doc = json!({ "spells": [{
        "def": { "id": "cx_dawn_hymn", "name": "Dawn Hymn", "description": "Light that burns.",
                 "spell_type": "damage", "damage_base": 90, "target_type": "enemy" },
        "meta": { "god_vnum": "god:sol", "author": "Saint Orin", "status": "faded" }
    }]})
    .to_string();
    let added = {
        let mut spells = h.state.lock().spell_definitions.clone();
        let report = ironmud::spellcraft::codex::seed_from_str(&h.db, &mut spells, &doc);
        h.state.lock().spell_definitions = spells;
        report.added
    };
    assert_eq!(added, vec!["cx_dawn_hymn".to_string()]);

    h.give_scroll("Seeker", "cx_dawn_hymn");
    let read = h.run("read", "scroll");
    assert!(read.contains("You have learned: Dawn Hymn"), "{read}");

    let codex = h.run("spells", "codex");
    assert!(codex.contains("Dawn Hymn"), "{codex}");
    assert!(codex.contains("Faded"), "{codex}");
    assert!(codex.contains("Saint Orin"), "{codex}");
    assert!(codex.contains("(known)"), "{codex}");
    assert!(
        codex.contains("a forgotten god"),
        "a god missing from this world reads as godless:\n{codex}"
    );

    let info = h.run("spells", "info dawn hymn");
    assert!(info.contains("An ancient working, set down by Saint Orin"), "{info}");
    assert!(info.contains("Mana:"), "a known spell shows its numbers:\n{info}");
}

#[test]
fn codex_listing_shows_faded_and_hides_lost_and_vetoed() {
    let mut h = Harness::new("Browser", false);
    h.add_codex("cx_bright", "Bright Spear", CanonStatus::Canon);
    h.add_codex("cx_dim", "Dim Spear", CanonStatus::Faded);
    h.add_codex("cx_gone", "Gone Spear", CanonStatus::Lost);
    h.add_codex("cx_banned", "Banned Spear", CanonStatus::Canon);
    ironmud::spellcraft::codex::set_veto(&h.db, &h.state, "cx_banned", true).unwrap();

    let out = h.run("spells", "codex");
    assert!(out.contains("Bright Spear") && out.contains("Canon"), "{out}");
    assert!(out.contains("Dim Spear") && out.contains("Faded"), "{out}");
    assert!(!out.contains("Gone Spear"), "lost spells are not named:\n{out}");
    assert!(out.contains("Rumoured, but lost: 1"), "{out}");
    assert!(!out.contains("Banned Spear"), "vetoed spells are hidden:\n{out}");

    let info = h.run("spells", "info gone spear");
    assert!(info.contains("You know of no spell"), "{info}");
    let unknown = h.run("spells", "info bright spear");
    assert!(unknown.contains("You do not know this spell"), "{unknown}");
    assert!(!unknown.contains("Mana:"), "numbers only for known spells:\n{unknown}");

    // The plain spellbook lists codex spells only once learned.
    let book = h.run("spells", "");
    assert!(
        !book.contains("Spear"),
        "unlearned codex spells stay out of the spellbook:\n{book}"
    );

    let filtered = h.run("spells", "codex nobody");
    assert!(filtered.contains("No spell of the gods matches"), "{filtered}");
}

#[test]
fn admin_codex_moderates_through_the_command() {
    let mut h = Harness::new("Warden", true);
    h.add_codex("cx_bright", "Bright Spear", CanonStatus::Canon);

    let list = h.run("admin", "codex list");
    assert!(list.contains("cx_bright") && list.contains("Bright Spear"), "{list}");

    let renamed = h.run("admin", "codex rename cx_bright Glory Spear");
    assert!(renamed.contains("now called Glory Spear"), "{renamed}");
    assert_eq!(h.state.lock().spell_definitions["cx_bright"].name, "Glory Spear");

    let status = h.run("admin", "codex status cx_bright lost");
    assert!(status.contains("is now lost"), "{status}");
    let veto = h.run("admin", "codex veto cx_bright");
    assert!(veto.contains("vetoed"), "{veto}");
    let show = h.run("admin", "codex show cx_bright");
    assert!(show.contains("VETOED") && show.contains("faded"), "{show}");

    let bad = h.run("admin", "codex status cx_nothing canon");
    assert!(bad.contains("No codex spell"), "{bad}");
    let seed = h.run("admin", "codex seed");
    assert!(seed.contains("Codex seed: 0 added"), "{seed}");
}

#[test]
fn spellcraft_settings_are_changeable_with_admin_config() {
    let mut h = Harness::new("Warden", true);
    let listing = h.run("admin", "config list");
    assert!(
        listing.contains("spellcraft.min_magic") && listing.contains("[10]"),
        "{listing}"
    );
    let set = h.run("admin", "config spellcraft.min_magic 12");
    assert!(set.contains("Set spellcraft.min_magic = 12"), "{set}");
    let on = h.run("admin", "config spellcraft.enabled on");
    assert!(on.contains("= true"), "{on}");
    let s = ironmud::spellcraft::Settings::load(&h.db);
    assert_eq!(s.min_magic, 12);
    assert!(s.enabled);
}

// ---------- S3: god configuration and the petition checklist ----------

#[test]
fn pray_petition_is_plain_prayer_while_spellcraft_is_off() {
    let mut h = Harness::new("Devout", false);
    h.god("god:sol", true);
    h.enter_temple();
    h.edit_live(|c| c.worship = Some(ironmud::types::WorshipState::new("god:sol", 0)));
    let out = h.run("pray", "petition");
    assert!(!out.contains("regards your petition"), "{out}");
    assert!(
        out.contains("devotion"),
        "falls through to ordinary temple prayer:\n{out}"
    );
}

#[test]
fn pray_petition_shows_the_checklist_to_worshippers() {
    let mut h = Harness::new("Devout", false);
    h.db.set_setting("spellcraft.enabled", "true").unwrap();
    h.god("god:sol", true);

    let godless = h.run("pray", "petition");
    assert!(godless.contains("You must first be bound to a god"), "{godless}");

    h.edit_live(|c| c.worship = Some(ironmud::types::WorshipState::new("god:sol", 0)));
    let outside = h.run("pray", "petition");
    assert!(outside.contains("only in a temple"), "{outside}");

    h.enter_temple();
    let out = h.run("pray", "petition");
    assert!(out.contains("Sol regards your petition"), "{out}");
    assert!(out.contains("[x]") && out.contains("Worship Sol"), "{out}");
    assert!(out.contains("Magic mastered") && out.contains("3 / 10"), "{out}");
    assert!(out.contains("[ ]"), "{out}");
    assert!(out.contains("(never)"), "{out}");

    // Progress shows up on the next look.
    h.edit_live(|c| {
        c.skills.insert(
            "magic".into(),
            ironmud::SkillProgress {
                level: 10,
                experience: 0,
            },
        );
    });
    let later = h.run("pray", "petition");
    assert!(later.contains("10 / 10"), "{later}");
}

#[test]
fn a_god_without_spellcraft_says_so() {
    let mut h = Harness::new("Devout", false);
    h.db.set_setting("spellcraft.enabled", "true").unwrap();
    h.god("god:sol", false);
    h.enter_temple();
    h.edit_live(|c| c.worship = Some(ironmud::types::WorshipState::new("god:sol", 0)));
    let out = h.run("pray", "petition");
    assert!(out.contains("grants no spells to mortals"), "{out}");
}

#[test]
fn builders_configure_spellcraft_with_medit() {
    let mut h = Harness::new("Maker", true);
    h.god("god:sol", false);
    let on = h.run("medit", "god:sol deity spellcraft enable");
    assert!(on.contains("now grants spells"), "{on}");
    let types = h.run("medit", "god:sol deity spellcraft types add damage");
    assert!(types.contains("Types:") && types.contains("damage"), "{types}");
    let bad = h.run("medit", "god:sol deity spellcraft types add utility");
    assert!(bad.contains("not valid"), "{bad}");
    let effect = h.run("medit", "god:sol deity spellcraft effects add sanctuary");
    assert!(
        effect.contains("damage_reduction"),
        "aliases are stored canonically:\n{effect}"
    );
    let favor = h.run("medit", "god:sol deity spellcraft favor 750");
    assert!(favor.contains("Min favor: 750"), "{favor}");
    let unlock = h.run("medit", "god:sol deity spellcraft unlock rite_of_dawn");
    assert!(unlock.contains("rite_of_dawn"), "{unlock}");

    let cfg =
        h.db.get_mobile_by_vnum("god:sol")
            .unwrap()
            .unwrap()
            .deity
            .unwrap()
            .spellcraft
            .unwrap();
    assert_eq!(cfg.allowed_types, vec!["damage"]);
    assert_eq!(cfg.allowed_effects, vec!["damage_reduction"]);
    assert_eq!(cfg.min_favor, Some(750));
    assert_eq!(cfg.unlock_quest_id.as_deref(), Some("rite_of_dawn"));

    let off = h.run("medit", "god:sol deity spellcraft disable");
    assert!(off.contains("no longer grants spells"), "{off}");
}

#[test]
fn help_pray_mentions_petition_only_to_worshippers_when_enabled() {
    let mut h = Harness::new("Devout", false);
    h.god("god:sol", true);
    h.edit_live(|c| c.worship = Some(ironmud::types::WorshipState::new("god:sol", 0)));
    assert!(
        !h.run("help", "pray").contains("pray petition"),
        "hidden while the system is off"
    );
    h.db.set_setting("spellcraft.enabled", "true").unwrap();
    assert!(h.run("help", "pray").contains("pray petition"));
    h.edit_live(|c| c.worship = None);
    assert!(
        !h.run("help", "pray").contains("pray petition"),
        "hidden from the godless"
    );
}

/// The deity editor read every argument one word too far to the right from
/// the day it shipped, so `deity setup god` saw "god" as the action. Pin the
/// pre-existing subcommands, not just the new spellcraft group.
#[test]
fn medit_deity_and_patron_subcommands_read_the_right_words() {
    let mut h = Harness::new("Maker", true);
    h.god("god:sol", false);
    let ep = h.run("medit", "god:sol deity epithet Lord of Noon");
    assert!(ep.contains("Deity epithet set to: Lord of Noon"), "{ep}");
    let tr = h.run("medit", "god:sol deity tribute interval 5");
    assert!(tr.contains("5 game day(s)"), "{tr}");
    let en = h.run("medit", "god:sol deity enemy add god:night");
    assert!(!en.contains("Usage") && !en.contains("Unknown"), "{en}");
    let cfg = h.db.get_mobile_by_vnum("god:sol").unwrap().unwrap().deity.unwrap();
    assert_eq!(cfg.epithet, "Lord of Noon");
    assert_eq!(cfg.tribute_interval_days, 5);
    assert_eq!(cfg.enemy_god_vnums, vec!["god:night".to_string()]);

    let mut priest = ironmud::MobileData::new("priest".to_string());
    priest.vnum = "priest:sol".into();
    priest.is_prototype = true;
    h.db.save_mobile_data(priest).unwrap();
    let pa = h.run("medit", "priest:sol patron god:sol");
    assert!(!pa.contains("Usage"), "{pa}");
    assert_eq!(
        h.db.get_mobile_by_vnum("priest:sol")
            .unwrap()
            .unwrap()
            .patron_god_vnum
            .as_deref(),
        Some("god:sol")
    );
}

// ---------- S4: drafting and offering ----------

impl Harness {
    /// Make the player an eligible worshipper of a fire god with a trial, in
    /// its temple, with spellcraft switched on.
    fn eligible_for_sol(&mut self) {
        self.db.set_setting("spellcraft.enabled", "true").unwrap();
        self.god("god:sol", true);
        let mut god = self.db.get_mobile_by_vnum("god:sol").unwrap().unwrap();
        let craft = god.deity.as_mut().unwrap().spellcraft.as_mut().unwrap();
        craft.allowed_types = vec!["damage".into()];
        craft.allowed_elements = vec!["fire".into()];
        craft.allowed_targets = vec!["enemy".into()];
        craft.trial_rules = vec![ironmud::types::TrialRule {
            quest_id: "rite_embers".into(),
            ..Default::default()
        }];
        self.db.save_mobile_data(god).unwrap();
        let mut q = ironmud::types::QuestData::new("rite_embers".into(), "Rite of Embers".into());
        q.repeatable = true;
        q.description = "Carry an ember from the forge to the altar.".into();
        self.db.save_quest_data(&q).unwrap();
        self.enter_temple();
        let today = ironmud::script::worship::current_absolute_day(&self.db);
        self.edit_live(|c| {
            let mut w = ironmud::types::WorshipState::new("god:sol", today);
            w.favor = 600;
            c.worship = Some(w);
            c.skills.insert(
                "magic".into(),
                ironmud::SkillProgress {
                    level: 10,
                    experience: 0,
                },
            );
            c.skills.insert(
                "spellcraft".into(),
                ironmud::SkillProgress {
                    level: 8,
                    experience: 0,
                },
            );
            for i in 0..5 {
                c.spell_progress.insert(
                    format!("s{i}"),
                    ironmud::SpellProgress {
                        level: 10,
                        experience: 0,
                    },
                );
            }
        });
    }

    fn mode(&self) -> Option<String> {
        self.connections.lock()[&self.conn_id].olc_mode.clone()
    }
}

#[test]
fn a_petition_is_drafted_offered_and_set_a_trial() {
    let mut h = Harness::new("Adept", false);
    h.eligible_for_sol();

    let open = h.run("pray", "petition");
    assert!(open.contains("Shape your gift"), "{open}");
    // Shape, element and reach have one answer each and fill themselves.
    assert!(open.contains("only way Sol allows"), "{open}");
    assert!(
        open.contains("How strong?") && open.contains("1) 80 damage (power 80, tier 3)"),
        "{open}"
    );
    assert_eq!(h.mode().as_deref(), Some("petitioning"));

    // Drafting words never reach the global parser: "north" is an answer.
    let bad = h.run("pray", "north");
    assert!(bad.contains("Choose one of the numbers"), "{bad}");

    let named = h.run("pray", "2");
    assert!(named.contains("By what name"), "{named}");
    let back = h.run("pray", "back");
    assert!(back.contains("How strong?"), "back reopens the last answer:\n{back}");
    h.run("pray", "2");
    let bad_name = h.run("pray", "X");
    assert!(bad_name.contains("3-32 characters"), "{bad_name}");
    h.run("pray", "Sun Lance");
    h.run("pray", "A lance of noon light.");
    h.run("pray", "Light gathers in your palm.");
    h.run("pray", "none");
    let review = h.run("pray", "none");
    assert!(
        review.contains("Sun Lance") && review.contains("Power:      90 (tier 3)"),
        "{review}"
    );
    assert!(
        review.contains("Price:") && review.contains("27 max HP pledged") && review.contains("250 favor"),
        "{review}"
    );
    assert!(
        review.contains("essence:fire x1 (quality 40+)") && review.contains("relic:god:sol x1"),
        "{review}"
    );
    assert!(review.contains("'offer'"), "{review}");

    let offered = h.run("pray", "offer");
    assert!(offered.contains("Sol accepts your petition for Sun Lance"), "{offered}");
    assert!(
        offered.contains("Rite of Embers") && offered.contains("Carry an ember"),
        "{offered}"
    );
    assert_eq!(h.mode(), None);
    let live = h.live();
    assert!(live.active_quests.contains_key("rite_embers"));
    let p = live.pending_petition.expect("frozen petition");
    assert_eq!(p.spell.name, "Sun Lance");
    assert_eq!(p.draft.flavor.caster, "Light gathers in your palm.");

    let again = h.run("pray", "petition");
    assert!(again.contains("awaits its trial: Rite of Embers"), "{again}");
}

#[test]
fn abort_leaves_the_petition_unspoken() {
    let mut h = Harness::new("Adept", false);
    h.eligible_for_sol();
    h.run("pray", "petition");
    let out = h.run("pray", "abort");
    assert!(out.contains("unspoken"), "{out}");
    assert_eq!(h.mode(), None);
    assert!(h.live().pending_petition.is_none());
}

#[test]
fn medit_sets_price_and_trials() {
    let mut h = Harness::new("Maker", true);
    h.god("god:sol", true);
    let p = h.run("medit", "god:sol deity spellcraft price weariness");
    assert!(p.contains("Price:     weariness"), "{p}");
    let t = h.run(
        "medit",
        "god:sol deity spellcraft trial add rite_embers type=damage element=fire power=100",
    );
    assert!(t.contains("1) rite_embers (damage, fire, power 100+)"), "{t}");
    let r = h.run("medit", "god:sol deity spellcraft trial remove 1");
    assert!(r.contains("(none: petitions will be refused"), "{r}");
}

#[test]
fn admin_pledges_and_marks_show_in_status() {
    let mut h = Harness::new("Warden", true);
    let p = h.run("admin", "pledge warden add hp=10 stamina=5 god=god:sol");
    assert!(p.contains("Pledge p1 taken"), "{p}");
    let m = h.run(
        "admin",
        "mark warden add god:sol hand regen_penalty:20 a sun-brand across the palm",
    );
    assert!(m.contains("Mark m1 stamped"), "{m}");

    let status = h.run("status", "");
    assert!(status.contains("Divine pledges:"), "{status}");
    assert!(status.contains("(-10 pledged)"), "{status}");
    assert!(status.contains("Weary"), "{status}");

    let detail = h.run("status", "detail");
    assert!(detail.contains("a sun-brand across the palm on their hand"), "{detail}");

    let faded = h.run("admin", "mark warden fade m1");
    assert_eq!(faded.trim(), "Done.");
    let help = h.run("admin", "help");
    assert!(help.contains("admin pledge <player>"), "{help}");
}

#[test]
fn petition_trial_ritual_push_seal_then_cast_the_new_spell() {
    use ironmud::spellcraft::ritual::{self, FixedDice};
    let mut h = Harness::new("Adept", false);
    h.eligible_for_sol();

    // Petition: a 90-power fire lance.
    h.run("pray", "petition");
    h.run("pray", "2");
    h.run("pray", "Sun Lance");
    h.run("pray", "A lance of noon light.");
    h.run("pray", "Light gathers in your palm.");
    h.run("pray", "none");
    h.run("pray", "none");
    let offered = h.run("pray", "offer");
    assert!(offered.contains("accepts your petition"), "{offered}");

    // The trial is done, and the components are in hand.
    h.edit_live(|c| {
        c.active_quests.remove("rite_embers");
        c.completed_quests.insert("rite_embers".into());
        c.mana_enabled = true;
        c.max_mana = 300;
        c.mana = 300;
        c.max_hp = 200;
        c.hp = 200;
    });
    let early = h.run("pray", "ritual");
    assert!(early.contains("do not carry"), "{early}");
    for (cat, q) in [("essence:fire", 50), ("relic:god:sol", 0)] {
        let mut item = ItemData::new("shard".into(), "a shard".into(), "A shard.".into());
        item.categories = vec![cat.into()];
        item.quality = q;
        let id = item.id;
        h.db.save_item_data(item).unwrap();
        h.db.move_item_to_inventory(&id, "Adept").unwrap();
    }
    let status = h.run("pray", "petition");
    assert!(status.contains("pray ritual"), "{status}");

    let begun = h.run("pray", "ritual");
    assert!(begun.contains("begin the rite of Sun Lance"), "{begun}");
    assert!(
        h.db.get_items_in_inventory("adept").unwrap().is_empty(),
        "components consumed"
    );
    let early_seal = h.run("pray", "seal");
    assert!(early_seal.contains("Wait for the rite"), "{early_seal}");

    // A stage passes; push (one risk die: a 4 holds), another stage, seal.
    let t0 = ritual::now();
    ritual::process_ritual_tick(&h.db, &h.connections, &h.state, t0 + 121, &mut FixedDice::new(&[]));
    let pause = h.run("pray", "petition");
    assert!(pause.contains("'pray seal'"), "{pause}");
    let help = h.run("help", "pray");
    assert!(help.contains("pray push"), "{help}");
    ritual::push(&h.db, &h.connections, "Adept", t0 + 122, &mut FixedDice::new(&[4]));
    ritual::process_ritual_tick(&h.db, &h.connections, &h.state, t0 + 250, &mut FixedDice::new(&[]));
    let sealed = h.run("pray", "seal");
    assert!(sealed.contains("Sun Lance is written into Sol's canon"), "{sealed}");
    assert!(sealed.contains("You bear a new mark"), "{sealed}");

    let live = h.live();
    assert!(live.learned_spells.contains(&"cx_sun_lance".to_string()));
    assert!(live.pending_petition.is_none() && live.pending_ritual.is_none());
    assert_eq!(live.mana, 300 - 25 - 25, "two stages drawn");
    let entry = h.db.get_codex("cx_sun_lance").unwrap().unwrap();
    assert_eq!(entry.meta.author.as_deref(), Some("Adept"));
    assert_eq!(entry.def.damage_base, 103);

    // Cast it at something.
    let mut dummy = ironmud::MobileData::new("a straw dummy".to_string());
    dummy.keywords = vec!["dummy".into(), "straw".into()];
    dummy.is_prototype = false;
    dummy.current_room_id = Some(live.current_room_id);
    dummy.current_hp = 500;
    dummy.max_hp = 500;
    let dummy_id = dummy.id;
    h.db.save_mobile_data(dummy).unwrap();
    h.db.move_mobile_to_room(&dummy_id, &live.current_room_id).unwrap();
    h.edit_live(|c| {
        c.combat.in_combat = true;
        c.combat.targets = vec![ironmud::types::CombatTarget {
            target_type: ironmud::types::CombatTargetType::Mobile,
            target_id: dummy_id,
            target_name: None,
        }];
    });
    let cast = h.run("cast", "sun dummy");
    assert!(
        cast.contains("You cast Sun Lance") && cast.contains("103 damage"),
        "{cast}"
    );
    // The author's own cast line, and the author's full form.
    assert!(cast.contains("Light gathers in your palm."), "{cast}");
    assert!(cast.contains("(true form)"), "{cast}");
}

#[test]
fn medit_sets_the_mark_pool() {
    let mut h = Harness::new("Maker", true);
    h.god("god:sol", true);
    let m = h.run(
        "medit",
        "god:sol deity spellcraft mark add hand strength_boost:2,dexterity_boost:-1 a sun-brand across the palm",
    );
    assert!(
        m.contains("1) a sun-brand across the palm (hand): strength_boost +2, dexterity_boost -1"),
        "{m}"
    );
    let bad = h.run("medit", "god:sol deity spellcraft mark add hand glow:3 a glow");
    assert!(bad.contains("Unknown effect 'glow'"), "{bad}");
    let r = h.run("medit", "god:sol deity spellcraft mark remove 1");
    assert!(r.contains("authors get a plain sigil"), "{r}");
}

#[test]
fn reading_another_gods_scroll_warns_then_confirm_teaches_it() {
    let mut h = Harness::new("Pupil", false);
    h.db.set_setting("spellcraft.enabled", "true").unwrap();
    h.god("god:sol", true);
    h.add_codex("cx_ember_lance", "Ember Lance", CanonStatus::Canon);
    h.give_scroll("Pupil", "cx_ember_lance");

    let warned = h.run("read", "scroll");
    assert!(warned.contains("a prayer of Sol"), "{warned}");
    assert!(warned.contains("read scroll confirm"), "{warned}");
    assert!(!h.live().learned_spells.contains(&"cx_ember_lance".to_string()));

    let learned = h.run("read", "scroll confirm");
    assert!(learned.contains("You have learned: Ember Lance"), "{learned}");
    let live = h.live();
    assert!(live.learned_spells.contains(&"cx_ember_lance".to_string()));
    assert_eq!(live.divine_grudges["god:sol"].count, 1);
}

#[test]
fn worship_renounce_warns_then_breaks_the_pact() {
    let mut h = Harness::new("Vex", false);
    h.db.set_setting("spellcraft.enabled", "true").unwrap();
    h.god("god:sol", true);
    h.enter_temple();
    h.edit_live(|c| c.worship = Some(ironmud::types::WorshipState::new("god:sol", 0)));

    let help = h.run("help", "worship");
    assert!(help.contains("worship renounce"), "{help}");

    let warned = h.run("worship", "renounce");
    assert!(warned.contains("You mean to renounce Sol"), "{warned}");
    assert!(h.live().worship.is_some());

    let done = h.run("worship", "renounce confirm");
    assert!(done.contains("You renounce Sol"), "{done}");
    let live = h.live();
    assert!(live.worship.is_none());
    assert!(
        live.former_faiths
            .iter()
            .any(|f| f.renounced && f.god_vnum == "god:sol")
    );

    let status = h.run("worship", "");
    assert!(
        status.contains("Gods who remember your offences") && status.contains("Sol (5"),
        "{status}"
    );
    let again = h.run("worship", "sol");
    assert!(again.contains("renounced a god too recently"), "{again}");
}

#[test]
fn craft_scroll_inks_a_known_spell_onto_a_blank() {
    let mut h = Harness::new("Scribe", false);
    h.db.set_setting("spellcraft.enabled", "true").unwrap();
    h.god("god:sol", true);
    h.add_codex("cx_ember_lance", "Ember Lance", CanonStatus::Canon);
    // Past Mira's monopoly window.
    h.db.update_codex("cx_ember_lance", |e| e.meta.sealed_day = -1000)
        .unwrap();
    h.edit_live(|c| {
        c.learned_spells.push("cx_ember_lance".into());
        c.skills.insert(
            "spellcraft".into(),
            ironmud::types::SkillProgress {
                level: 5,
                experience: 0,
            },
        );
        c.mana = 200;
    });
    let mut blank = ItemData::new(
        "blank scroll".to_string(),
        "a blank scroll".to_string(),
        "A blank scroll lies here.".to_string(),
    );
    blank.categories = vec!["blank_scroll".into()];
    let id = blank.id;
    h.db.save_item_data(blank).unwrap();
    h.db.move_item_to_inventory(&id, "Scribe").unwrap();

    let help = h.run("help", "craft");
    assert!(help.contains("craft scroll <spell>"), "{help}");

    let out = h.run("craft", "scroll ember lance");
    assert!(out.contains("You ink Ember Lance"), "{out}");
    let scroll = h.db.get_item_data(&id).unwrap().unwrap();
    assert_eq!(scroll.teaches_spell.as_deref(), Some("cx_ember_lance"));
    assert_eq!(h.live().mana, 160);

    let none = h.run("craft", "scroll ember lance");
    assert!(none.contains("You need a blank scroll"), "{none}");
}

//! Dialogue tree slice 1 integration tests.
//!
//! Type-shape and serde round-trip coverage; deeper engine behavior
//! (condition/effect application, visible_choices filtering, current_node
//! resolution) is exercised by the in-module unit tests in
//! `src/script/dialogue.rs`.

#![recursion_limit = "256"]

use ironmud::dialogue_edit;
use ironmud::types::{
    CharacterData, DgScope, DialogueChoice, DialogueCondition, DialogueEffect, DialogueNode,
    DialoguePairState, DialogueTarget, DialogueTree, FlagScope, MobileData,
};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn dialogue_tree_round_trips_through_json() {
    let mut nodes = HashMap::new();
    nodes.insert(
        "root".to_string(),
        DialogueNode {
            text: "Greetings, traveler.".into(),
            choices: vec![DialogueChoice {
                keyword: "quest".into(),
                label: "Tell me about the quest".into(),
                target: DialogueTarget::Goto {
                    node: "quest_offer".into(),
                },
                conditions: vec![DialogueCondition::FlagUnset {
                    name: "quest_done".into(),
                    scope: FlagScope::Local,
                }],
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
        "quest_offer".to_string(),
        DialogueNode {
            text: "Bring me ten rat tails.".into(),
            choices: vec![DialogueChoice {
                keyword: "accept".into(),
                label: "Accept the quest".into(),
                target: DialogueTarget::Exit,
                conditions: vec![DialogueCondition::SkillAtLeast {
                    key: "diplomacy".into(),
                    level: 1,
                }],
                effects: vec![
                    DialogueEffect::SetFlag {
                        name: "quest_started".into(),
                        scope: FlagScope::Local,
                    },
                    DialogueEffect::SetCounter {
                        key: "quest.rat_target".into(),
                        value: 10,
                    },
                    DialogueEffect::AwardSkillXp {
                        skill: "diplomacy".into(),
                        amount: 25,
                    },
                    DialogueEffect::FireDgTrigger {
                        trigger_type: "on_receive".into(),
                        arg: "".into(),
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
    let tree = DialogueTree {
        root_node: "root".into(),
        nodes,
    };
    let json = serde_json::to_string(&tree).expect("serialize");
    let back: DialogueTree = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.root_node, "root");
    assert_eq!(back.nodes.len(), 2);
    let root = &back.nodes["root"];
    let q = &root.choices[0];
    assert_eq!(q.keyword, "quest");
    let quest_offer = &back.nodes["quest_offer"];
    let accept = &quest_offer.choices[0];
    match &accept.target {
        DialogueTarget::Exit => {}
        _ => panic!("expected Exit target"),
    }
    assert_eq!(accept.effects.len(), 4);
}

#[test]
fn dialogue_target_kinds_serialize_with_tag_field() {
    let goto = DialogueTarget::Goto {
        node: "next".into(),
    };
    let exit = DialogueTarget::Exit;
    let repeat = DialogueTarget::Repeat;
    assert_eq!(
        serde_json::to_value(&goto).unwrap(),
        serde_json::json!({"kind": "goto", "node": "next"})
    );
    assert_eq!(
        serde_json::to_value(&exit).unwrap(),
        serde_json::json!({"kind": "exit"})
    );
    assert_eq!(
        serde_json::to_value(&repeat).unwrap(),
        serde_json::json!({"kind": "repeat"})
    );
}

#[test]
fn condition_kinds_round_trip_through_json() {
    let conditions = vec![
        DialogueCondition::FlagSet {
            name: "asked".into(),
            scope: FlagScope::Local,
        },
        DialogueCondition::FlagUnset {
            name: "saved_village".into(),
            scope: FlagScope::Global,
        },
        DialogueCondition::HasItem {
            vnum: "5023".into(),
            qty: 2,
        },
        DialogueCondition::SkillAtLeast {
            key: "elvish".into(),
            level: 5,
        },
        DialogueCondition::CounterAtLeast {
            key: "kills.dragons".into(),
            value: 1,
        },
        DialogueCondition::DgVarEquals {
            scope: DgScope::Player,
            key: "faction".into(),
            value: "guild".into(),
        },
    ];
    let json = serde_json::to_string(&conditions).expect("serialize");
    let back: Vec<DialogueCondition> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.len(), 6);
}

#[test]
fn effect_kinds_round_trip_through_json() {
    let effects = vec![
        DialogueEffect::SetFlag {
            name: "tipped".into(),
            scope: FlagScope::Local,
        },
        DialogueEffect::ClearFlag {
            name: "asked".into(),
            scope: FlagScope::Local,
        },
        DialogueEffect::GiveItem {
            vnum: "5023".into(),
            qty: 1,
        },
        DialogueEffect::TakeItem {
            vnum: "5024".into(),
            qty: 3,
        },
        DialogueEffect::AwardSkillXp {
            skill: "diplomacy".into(),
            amount: 50,
        },
        DialogueEffect::SetCounter {
            key: "quest.progress".into(),
            value: 1,
        },
        DialogueEffect::IncrementCounter {
            key: "quest.progress".into(),
            by: 1,
        },
        DialogueEffect::SetDgVar {
            scope: DgScope::Mob,
            key: "asked".into(),
            value: "1".into(),
        },
        DialogueEffect::FireDgTrigger {
            trigger_type: "on_receive".into(),
            arg: "complete".into(),
        },
    ];
    let json = serde_json::to_string(&effects).expect("serialize");
    let back: Vec<DialogueEffect> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.len(), 9);
}

#[test]
fn flag_scope_defaults_to_local() {
    let json = r#"{"kind": "flag_set", "name": "x"}"#;
    let cond: DialogueCondition = serde_json::from_str(json).expect("parse");
    match cond {
        DialogueCondition::FlagSet { scope, .. } => assert_eq!(scope, FlagScope::Local),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn dialogue_pair_state_default_has_no_node() {
    let s = DialoguePairState::default();
    assert!(s.current_node.is_none());
    assert_eq!(s.last_seen_secs, 0);
}

#[test]
fn character_data_dialogue_fields_default_empty() {
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build legacy character");
    assert!(
        ch.dialogue_pair_state.is_empty(),
        "missing field must default empty"
    );
    assert!(ch.dialogue_flags.is_empty());
}

#[test]
fn mobile_data_dialogue_tree_is_optional_and_defaults_none() {
    let m = MobileData::new("npc".into());
    assert!(m.dialogue_tree.is_none(), "new mob has no tree by default");
    // Round-trip through JSON drops the field via skip_serializing_if; missing
    // field deserializes back to None.
    let json = serde_json::to_string(&m).expect("serialize");
    assert!(
        !json.contains("dialogue_tree"),
        "default tree should be omitted"
    );
    let back: MobileData = serde_json::from_str(&json).expect("deserialize");
    assert!(back.dialogue_tree.is_none());
}

#[test]
fn dialogue_choice_round_trips_slice3_fields() {
    // Explicit JSON with all three new fields set.
    let json = serde_json::json!({
        "keyword": "secret",
        "label": "Ask about the smith",
        "target": { "kind": "exit" },
        "hint": "You sense she might say more if you'd ever sailed.",
        "cooldown_secs": 90,
        "once_per_player": true,
    });
    let c: DialogueChoice = serde_json::from_value(json).expect("deserialize");
    assert_eq!(c.hint.as_deref(), Some("You sense she might say more if you'd ever sailed."));
    assert_eq!(c.cooldown_secs, Some(90));
    assert!(c.once_per_player);
    let back = serde_json::to_value(&c).expect("serialize");
    assert_eq!(back["hint"], "You sense she might say more if you'd ever sailed.");
    assert_eq!(back["cooldown_secs"], 90);
    assert_eq!(back["once_per_player"], true);

    // Default JSON without new fields stays byte-clean — missing fields
    // deserialize to None/false, and serialization back drops them.
    let plain = serde_json::json!({
        "keyword": "bye",
        "label": "Farewell",
        "target": { "kind": "exit" },
    });
    let c2: DialogueChoice = serde_json::from_value(plain).expect("deserialize");
    assert!(c2.hint.is_none());
    assert!(c2.cooldown_secs.is_none());
    assert!(!c2.once_per_player);
    let back = serde_json::to_string(&c2).expect("serialize");
    assert!(!back.contains("hint"), "absent fields should not serialize");
    assert!(!back.contains("cooldown_secs"));
    assert!(!back.contains("once_per_player"));
}

#[test]
fn dialogue_tree_attaches_to_mobile_data() {
    let mut m = MobileData::new("barkeep".into());
    m.vnum = "3001".into();
    let mut nodes = HashMap::new();
    nodes.insert(
        "root".to_string(),
        DialogueNode {
            text: "Hi.".into(),
            choices: vec![],
            on_enter: vec![],
            on_each_visit: vec![],
            on_exit: vec![],
        },
    );
    m.dialogue_tree = Some(DialogueTree {
        root_node: "root".into(),
        nodes,
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MobileData = serde_json::from_str(&json).expect("deserialize");
    assert!(back.dialogue_tree.is_some());
    assert_eq!(back.dialogue_tree.as_ref().unwrap().nodes.len(), 1);
}

#[test]
fn granular_helpers_compose_a_full_tree_via_db() {
    use ironmud::db::Db;

    let path = format!("test_dialogue_granular_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&path).expect("open db");

        // Save a fresh prototype mob with no tree.
        let mut mob = MobileData::new("greeter".into());
        mob.vnum = "9100".into();
        let mob_id = mob.id;
        db.save_mobile_data(mob).expect("save");

        // Helper: load → mutate → save.
        let apply = |op: &dyn Fn(&mut Option<DialogueTree>) -> Result<(), dialogue_edit::DialogueEditError>| {
            let mut m = db.get_mobile_data(&mob_id).unwrap().unwrap();
            op(&mut m.dialogue_tree).expect("op ok");
            db.save_mobile_data(m).expect("save");
        };

        // 1. First add_node auto-initializes the tree with the new node as root.
        // We bypass the automatic Rhai/HTTP flow and exercise the helpers
        // directly; the auto-init logic mirrors what the API handler does.
        {
            let mut m = db.get_mobile_data(&mob_id).unwrap().unwrap();
            assert!(m.dialogue_tree.is_none());
            dialogue_edit::ensure_initialized(&mut m.dialogue_tree, "Hello traveler.");
            // Rename root to "greet" — mimics the API handler when caller
            // requests their own root name.
            if let Some(t) = m.dialogue_tree.as_mut() {
                if let Some(n) = t.nodes.remove("root") {
                    t.nodes.insert("greet".into(), n);
                    t.root_node = "greet".into();
                }
            }
            db.save_mobile_data(m).expect("save");
        }

        // 2. Add a "shop" node.
        apply(&|slot| {
            dialogue_edit::add_node(
                slot,
                "shop",
                DialogueNode {
                    text: "Wares laid out.".into(),
                    choices: vec![],
                    on_enter: vec![],
                    on_each_visit: vec![],
                    on_exit: vec![],
                },
            )
        });

        // 3. Add a choice on "greet" that goes to "shop".
        apply(&|slot| {
            dialogue_edit::add_choice(
                slot,
                "greet",
                DialogueChoice {
                    keyword: "shop".into(),
                    label: "View wares".into(),
                    target: DialogueTarget::Goto { node: "shop".into() },
                    conditions: vec![],
                    effects: vec![],
                    hint: None,
                    cooldown_secs: None,
                    once_per_player: false,
                },
            )
        });

        // 4. Removing "shop" while it's referenced must fail.
        {
            let mut m = db.get_mobile_data(&mob_id).unwrap().unwrap();
            let err = dialogue_edit::remove_node(&mut m.dialogue_tree, "shop").unwrap_err();
            assert!(matches!(
                err,
                dialogue_edit::DialogueEditError::NodeReferenced(_, _)
            ));
        }

        // 5. Patch the greet node's on_exit to set a flag.
        apply(&|slot| {
            dialogue_edit::update_node(
                slot,
                "greet",
                dialogue_edit::NodePatch {
                    text: None,
                    on_enter: None,
                    on_each_visit: None,
                    on_exit: Some(vec![DialogueEffect::SetFlag {
                        name: "said_hi".into(),
                        scope: FlagScope::Local,
                    }]),
                },
            )
        });

        // 6. Re-load and verify the persisted shape.
        let final_mob = db.get_mobile_data(&mob_id).unwrap().unwrap();
        let tree = final_mob.dialogue_tree.expect("tree");
        assert_eq!(tree.root_node, "greet");
        assert_eq!(tree.nodes.len(), 2);
        let greet = &tree.nodes["greet"];
        assert_eq!(greet.choices.len(), 1);
        assert_eq!(greet.choices[0].keyword, "shop");
        assert_eq!(greet.on_exit.len(), 1);
    }));

    let _ = std::fs::remove_dir_all(&path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// src/script/social.rs
// Rhai API for the relationship + happiness system.
//
// Read-only likes/dislikes getters for medit display and ambient emote logic;
// setters for happiness and affinity that only operate on simulated mobiles
// (those with `social = Some(_)`).

use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;

use crate::db::Db;
use crate::types::{MobileData, MoodState, RelationshipKind};

/// Build the examine-time social cue string for a mobile. Combines at most
/// one mood hint, one bereavement hint, and one cohabitant hint — enough
/// context to roleplay against without overwhelming the room description.
///
/// Separated from the Rhai closure so it can be unit-tested without spinning
/// up an engine. The caller is expected to have already verified that the
/// mobile has a `SocialState`.
pub fn build_social_cues(
    db: &Db,
    mobile: &MobileData,
    social: &crate::types::SocialState,
    current_game_day: i32,
) -> String {
    let mut cues: Vec<String> = Vec::new();

    // Life-stage hint: only surface non-adult stages to keep the line
    // budget for the emotionally-salient cues below.
    if let Some(chars) = mobile.characteristics.as_ref() {
        use crate::types::{life_stage_for_age, LifeStage};
        match life_stage_for_age(chars.age) {
            LifeStage::Baby => cues.push("They are barely more than an infant.".into()),
            LifeStage::Child => cues.push("They have a child's easy wonder.".into()),
            LifeStage::Adolescent => cues.push("They carry themselves with adolescent energy.".into()),
            LifeStage::Elderly => cues.push("Their frame bears the marks of many years.".into()),
            _ => {}
        }
    }

    // Mood is the most visible cue; bereavement emotes mention grief
    // specifically, so keep the mood line as a general emotional read.
    match social.mood {
        MoodState::Breakdown => cues.push("They look on the edge of collapse.".into()),
        MoodState::Depressed => cues.push("Their face is drawn, eyes dull.".into()),
        MoodState::Sad => cues.push("They seem quietly unhappy.".into()),
        MoodState::Content => cues.push("They carry themselves with quiet contentment.".into()),
        MoodState::Normal => {}
    }

    // Prefer a specific bereavement note ("mourning their father") when
    // we have one. Falls back to the generic "recent loss" line only if
    // the mourning window is active but we've lost the specific notes
    // (e.g. legacy save before C1).
    let active_note = social
        .bereaved_for
        .iter()
        .filter(|n| n.until_day > current_game_day)
        .max_by_key(|n| n.until_day);
    if let Some(n) = active_note {
        let role_line = match n.kind {
            RelationshipKind::Partner => "They are in mourning for their partner.",
            RelationshipKind::Parent => "They are in mourning for their parent.",
            RelationshipKind::Child => "They are in mourning for their child.",
            RelationshipKind::Sibling => "They are in mourning for their sibling.",
            RelationshipKind::Cohabitant => "A recent loss weighs heavily on them.",
            RelationshipKind::Friend => "A recent loss weighs heavily on them.",
        };
        cues.push(role_line.to_string());
    } else if social
        .bereaved_until_day
        .map(|d| d > current_game_day)
        .unwrap_or(false)
    {
        cues.push("A recent loss weighs heavily on them.".into());
    }

    // Family in the same room — surface the most-salient relation to hint
    // at kinship without flooding the description. Priority: Partner →
    // Parent → Child → Sibling. Cohabitant is handled below with its own
    // warmer line.
    let family_pri = [
        (RelationshipKind::Partner, "partner"),
        (RelationshipKind::Parent, "parent"),
        (RelationshipKind::Child, "child"),
        (RelationshipKind::Sibling, "sibling"),
    ];
    'family: for (kind, role) in family_pri {
        for rel in &mobile.relationships {
            if rel.kind != kind {
                continue;
            }
            if let Ok(Some(other)) = db.get_mobile_data(&rel.other_id) {
                if other.current_hp <= 0 {
                    continue;
                }
                let same_room = mobile.current_room_id.is_some()
                    && mobile.current_room_id == other.current_room_id;
                if same_room {
                    cues.push(format!("Their {} {} is here.", role, other.name));
                    break 'family;
                }
            }
        }
    }

    // Cohabitant hint: keep the original warm lines. Only runs if the
    // mobile has a Cohabitant (not every pair bond — family kinds handled
    // above).
    if let Some(cohab_rel) = mobile
        .relationships
        .iter()
        .find(|r| matches!(r.kind, RelationshipKind::Cohabitant))
    {
        if let Ok(Some(partner)) = db.get_mobile_data(&cohab_rel.other_id) {
            if partner.current_hp > 0 {
                let same_room = mobile.current_room_id.is_some()
                    && mobile.current_room_id == partner.current_room_id;
                if same_room {
                    cues.push(format!("They glance warmly at {}.", partner.name));
                } else {
                    cues.push(format!(
                        "They occasionally look off as if thinking of {}.",
                        partner.name
                    ));
                }
            }
        }
    }

    cues.join(" ")
}

pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // get_mobile_happiness(mobile_id) -> i64 (0-100, returns -1 if not simulated)
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_happiness", move |mobile_id: String| -> i64 {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return -1,
        };
        match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m.social.map(|s| s.happiness as i64).unwrap_or(-1),
            _ => -1,
        }
    });

    // set_mobile_happiness(mobile_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_happiness",
        move |mobile_id: String, value: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let clamped = (value as i32).clamp(0, 100);
            let ok = cloned_db
                .update_mobile(&uuid, |m| {
                    if let Some(s) = m.social.as_mut() {
                        s.happiness = clamped;
                        crate::social::apply_mood(m);
                    }
                })
                .is_ok();
            ok
        },
    );

    // adjust_mobile_happiness(mobile_id, delta) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "adjust_mobile_happiness",
        move |mobile_id: String, delta: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db
                .update_mobile(&uuid, |m| {
                    if let Some(s) = m.social.as_mut() {
                        s.happiness = (s.happiness + delta as i32).clamp(0, 100);
                        crate::social::apply_mood(m);
                    }
                })
                .is_ok()
        },
    );

    // get_mobile_mood(mobile_id) -> String (returns "" if not simulated)
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_mood", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m
                .social
                .map(|s| s.mood.to_display_string().to_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    });

    // get_mobile_likes(mobile_id) -> Array<String>
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_likes", move |mobile_id: String| -> Array {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Array::new(),
        };
        match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m
                .social
                .map(|s| s.likes.into_iter().map(Dynamic::from).collect())
                .unwrap_or_default(),
            _ => Array::new(),
        }
    });

    // get_mobile_dislikes(mobile_id) -> Array<String>
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_dislikes", move |mobile_id: String| -> Array {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Array::new(),
        };
        match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m
                .social
                .map(|s| s.dislikes.into_iter().map(Dynamic::from).collect())
                .unwrap_or_default(),
            _ => Array::new(),
        }
    });

    // get_mobile_affinity(source_id, target_id) -> i64 (0 if no relationship)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_affinity",
        move |source_id: String, target_id: String| -> i64 {
            let src = match uuid::Uuid::parse_str(&source_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };
            let tgt = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };
            match cloned_db.get_mobile_data(&src) {
                Ok(Some(m)) => m
                    .relationships
                    .iter()
                    .find(|r| r.other_id == tgt)
                    .map(|r| r.affinity as i64)
                    .unwrap_or(0),
                _ => 0,
            }
        },
    );

    // set_mobile_affinity(source_id, target_id, value) -> bool
    // Upserts a Relationship entry (kind=Friend if new) and clamps to [-100, 100].
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_affinity",
        move |source_id: String, target_id: String, value: i64| -> bool {
            use crate::types::{Relationship, RelationshipKind};
            let src = match uuid::Uuid::parse_str(&source_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let tgt = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let clamped = (value as i32).clamp(-100, 100);
            cloned_db
                .update_mobile(&src, |m| {
                    if m.social.is_none() {
                        return;
                    }
                    if let Some(rel) = m.relationships.iter_mut().find(|r| r.other_id == tgt) {
                        rel.affinity = clamped;
                    } else {
                        m.relationships.push(Relationship {
                            other_id: tgt,
                            kind: RelationshipKind::Friend,
                            affinity: clamped,
                            last_interaction_day: 0,
                            recent_topics: Vec::new(),
                        });
                    }
                })
                .map(|opt| opt.is_some())
                .unwrap_or(false)
        },
    );

    // get_npc_social_cues(mobile_id) -> String
    // One-line (or empty) roleplay hint layered on top of the needs-based
    // visual cues. Surfaces life stage, mood, bereavement, family presence,
    // and cohabitants so players get a sense of the NPC's inner life via
    // `examine`. Works for non-simulated mobiles too (juvenile family
    // members still get life-stage + "their parent is here" cues) — only
    // mood and bereavement cues require a SocialState.
    let cloned_db = db.clone();
    engine.register_fn("get_npc_social_cues", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return String::new(),
        };
        // Non-simulated mobiles (migrant children, prototypes) have no
        // SocialState — synthesize a neutral one so life-stage/family cues
        // still flow through build_social_cues.
        let social = mobile
            .social
            .as_ref()
            .cloned()
            .unwrap_or_else(crate::types::SocialState::default);
        let current_game_day = cloned_db
            .get_game_time()
            .ok()
            .map(|gt| crate::migration::absolute_game_day(gt.year, gt.month, gt.day) as i32)
            .unwrap_or(0);
        build_social_cues(&cloned_db, &mobile, &social, current_game_day)
    });

    // describe_mobile_relationship(subject_id, target_keyword) -> Map or ()
    // Returns `#{name, affinity, kind}` for the first mobile in `subject_id`'s
    // `relationships` whose keywords or name match `target_keyword`. Used by
    // the player-facing `ask` command to answer "how does X feel about Y".
    // Returns Unit if the subject is not simulated or has no matching acquaintance.
    let cloned_db = db.clone();
    engine.register_fn(
        "describe_mobile_relationship",
        move |subject_id: String, target_keyword: String| -> Dynamic {
            let src = match uuid::Uuid::parse_str(&subject_id) {
                Ok(u) => u,
                Err(_) => return Dynamic::UNIT,
            };
            let subject = match cloned_db.get_mobile_data(&src) {
                Ok(Some(m)) => m,
                _ => return Dynamic::UNIT,
            };
            let needle = target_keyword.to_lowercase();
            if needle.is_empty() {
                return Dynamic::UNIT;
            }
            for rel in &subject.relationships {
                let other = match cloned_db.get_mobile_data(&rel.other_id) {
                    Ok(Some(m)) => m,
                    _ => continue,
                };
                let name_match = other.name.to_lowercase().contains(&needle);
                let keyword_match = other.keywords.iter().any(|k| k.to_lowercase() == needle);
                if !name_match && !keyword_match {
                    continue;
                }
                let mut map = Map::new();
                map.insert("name".into(), Dynamic::from(other.name.clone()));
                map.insert("other_id".into(), Dynamic::from(rel.other_id.to_string()));
                map.insert("affinity".into(), Dynamic::from(rel.affinity as i64));
                map.insert(
                    "kind".into(),
                    Dynamic::from(rel.kind.to_display_string().to_string()),
                );
                return Dynamic::from(map);
            }
            Dynamic::UNIT
        },
    );

    // ---------------------------------------------------------------------
    // Family relationship API
    //
    // The family-kind variants (Partner/Parent/Child/Sibling) exist on
    // RelationshipKind but were builder-invisible until now. These bindings
    // let medit wire families for testing and are reused by the migrant
    // family spawn path. Each binding writes BOTH directions so the two
    // mobiles stay mutually consistent — a dangling one-sided family tie
    // would confuse grief + cues code.
    // ---------------------------------------------------------------------

    // set_family_relationship(source_id, target_id, kind_str) -> bool
    // Valid `kind_str`: "parent", "child", "sibling", "partner".
    // - Parent: source is parent of target; target becomes child of source.
    // - Child:  source is child of target;  target becomes parent of source.
    // - Sibling: symmetric.
    // - Partner: symmetric. Monogamy enforced: both sides must be Partner-free
    //   (or already linked to each other) or the call fails.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_family_relationship",
        move |source_id: String, target_id: String, kind_str: String| -> bool {
            let src = match uuid::Uuid::parse_str(&source_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let tgt = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            crate::social::set_family_relationship(&cloned_db, src, tgt, &kind_str).is_ok()
        },
    );

    // unset_family_relationship(source_id, target_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "unset_family_relationship",
        move |source_id: String, target_id: String| -> bool {
            let src = match uuid::Uuid::parse_str(&source_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let tgt = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            crate::social::unset_family_relationship(&cloned_db, src, tgt)
        },
    );

    // get_mobile_family(mobile_id) -> Array<Map>
    // Returns only family-kind relationships (Partner/Parent/Child/Sibling).
    // Cohabitant is excluded — it's a housing status, not kinship.
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_family", move |mobile_id: String| -> Array {
        use crate::types::RelationshipKind;
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Array::new(),
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return Array::new(),
        };
        mobile
            .relationships
            .iter()
            .filter(|r| {
                matches!(
                    r.kind,
                    RelationshipKind::Partner
                        | RelationshipKind::Parent
                        | RelationshipKind::Child
                        | RelationshipKind::Sibling
                )
            })
            .map(|r| {
                let name = cloned_db
                    .get_mobile_data(&r.other_id)
                    .ok()
                    .flatten()
                    .map(|m| m.name)
                    .unwrap_or_else(|| "<missing>".to_string());
                let mut map = Map::new();
                map.insert("other_id".into(), Dynamic::from(r.other_id.to_string()));
                map.insert("name".into(), Dynamic::from(name));
                map.insert(
                    "kind".into(),
                    Dynamic::from(r.kind.to_display_string().to_string()),
                );
                map.insert("affinity".into(), Dynamic::from(r.affinity as i64));
                Dynamic::from(map)
            })
            .collect()
    });

    // new_household(mobile_id) -> String (the new household id as a string)
    // Mints a fresh household id, assigns it, and returns the id. Returns ""
    // on failure.
    let cloned_db = db.clone();
    engine.register_fn("new_household", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let fresh = uuid::Uuid::new_v4();
        let ok = cloned_db
            .update_mobile(&uuid, |m| {
                m.household_id = Some(fresh);
            })
            .ok()
            .flatten()
            .is_some();
        if ok { fresh.to_string() } else { String::new() }
    });

    // link_household(source_id, other_id) -> bool
    // Copies `other_id`'s household_id onto `source_id`. If other lacks one,
    // mint a fresh household and assign it to BOTH so the pair shares one.
    let cloned_db = db.clone();
    engine.register_fn(
        "link_household",
        move |source_id: String, other_id: String| -> bool {
            let src = match uuid::Uuid::parse_str(&source_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let tgt = match uuid::Uuid::parse_str(&other_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let other = match cloned_db.get_mobile_data(&tgt) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let household = other.household_id.unwrap_or_else(uuid::Uuid::new_v4);
            let a = cloned_db
                .update_mobile(&src, |m| m.household_id = Some(household))
                .ok()
                .flatten()
                .is_some();
            // If other had no household, give it one too.
            let b = if other.household_id.is_none() {
                cloned_db
                    .update_mobile(&tgt, |m| m.household_id = Some(household))
                    .ok()
                    .flatten()
                    .is_some()
            } else {
                true
            };
            a && b
        },
    );

    // clear_household(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_household", move |mobile_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db
            .update_mobile(&uuid, |m| m.household_id = None)
            .ok()
            .flatten()
            .is_some()
    });

    // ---------------------------------------------------------------------
    // Pregnancy API (builder-facing, used by medit)
    // ---------------------------------------------------------------------

    // get_pregnancy_status(mobile_id) -> Map or ()
    // Returns #{pregnant_until_day, pregnant_by, days_remaining} when the
    // mobile is a simulated female with active pregnancy. Returns Unit
    // otherwise — medit surfaces that as "not pregnant".
    let cloned_db = db.clone();
    engine.register_fn("get_pregnancy_status", move |mobile_id: String| -> Dynamic {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Dynamic::UNIT,
        };
        let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) else {
            return Dynamic::UNIT;
        };
        let Some(social) = mobile.social.as_ref() else { return Dynamic::UNIT };
        let Some(due) = social.pregnant_until_day else { return Dynamic::UNIT };
        let today = cloned_db
            .get_game_time()
            .ok()
            .map(|gt| crate::migration::absolute_game_day(gt.year, gt.month, gt.day) as i32)
            .unwrap_or(0);
        let mut map = Map::new();
        map.insert("pregnant_until_day".into(), Dynamic::from(due as i64));
        map.insert("days_remaining".into(), Dynamic::from((due - today) as i64));
        if let Some(fid) = social.pregnant_by {
            map.insert("pregnant_by".into(), Dynamic::from(fid.to_string()));
        }
        Dynamic::from(map)
    });

    // force_pregnancy(mobile_id, father_id, gestation_days) -> bool
    // Builder-only debug hook. Sets `pregnant_until_day = today + gestation_days`
    // and `pregnant_by = father_id` regardless of eligibility. Pass
    // gestation_days <= 0 to use the default.
    let cloned_db = db.clone();
    engine.register_fn(
        "force_pregnancy",
        move |mobile_id: String, father_id: String, gestation_days: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let father = match uuid::Uuid::parse_str(&father_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let today = cloned_db
                .get_game_time()
                .ok()
                .map(|gt| crate::migration::absolute_game_day(gt.year, gt.month, gt.day) as i32)
                .unwrap_or(0);
            let days = if gestation_days > 0 {
                gestation_days as i32
            } else {
                crate::aging::PREGNANCY_GESTATION_DAYS
            };
            cloned_db
                .update_mobile(&uuid, |m| {
                    if let Some(s) = m.social.as_mut() {
                        s.pregnant_until_day = Some(today + days);
                        s.pregnant_by = Some(father);
                    }
                })
                .ok()
                .flatten()
                .map(|m| m.social.as_ref().and_then(|s| s.pregnant_until_day).is_some())
                .unwrap_or(false)
        },
    );

    // force_birth(mobile_id) -> bool
    // Immediately triggers spawn_child, mirroring what the aging tick does
    // on the due day. Fails if the mobile isn't pregnant or spawn_child
    // errors (e.g. area not resolvable).
    let cloned_db = db.clone();
    engine.register_fn("force_birth", move |mobile_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mother = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        let father_id = mother.social.as_ref().and_then(|s| s.pregnant_by);
        // load_migration_data is cheap enough for a builder-issued command.
        let data = match crate::migration::load_migration_data(std::path::Path::new("scripts/data")) {
            Ok(d) => d,
            Err(_) => return false,
        };
        crate::migration::spawn_child(&cloned_db, &data, uuid, father_id).is_ok()
    });

    // clear_pregnancy(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_pregnancy", move |mobile_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db
            .update_mobile(&uuid, |m| {
                if let Some(s) = m.social.as_mut() {
                    s.pregnant_until_day = None;
                    s.pregnant_by = None;
                }
            })
            .ok()
            .flatten()
            .is_some()
    });
}

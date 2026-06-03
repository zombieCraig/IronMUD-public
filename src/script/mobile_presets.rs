// src/script/mobile_presets.rs
// Generic mobile preset system: stamp a bundle of flags + stats + on-hit
// effects onto an existing mobile from a JSON-defined preset. Reused for any
// archetype (guards, beasts, vampires, …); the data lives in
// `scripts/data/mobile_presets.json` and is read fresh on each call so live
// edits take effect without a restart.

use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

const PRESETS_PATH: &str = "scripts/data/mobile_presets.json";

fn load_presets() -> Vec<serde_json::Value> {
    let data = match std::fs::read_to_string(PRESETS_PATH) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn preset_has_tag(preset: &serde_json::Value, tag: &str) -> bool {
    preset
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|t| t.as_str() == Some(tag)))
        .unwrap_or(false)
}

pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // list_mobile_presets(tag_filter) -> Array<Map>
    // Pass an empty string to list all presets; otherwise filter by tag.
    engine.register_fn(
        "list_mobile_presets",
        |tag_filter: String| -> rhai::Array {
            let presets = load_presets();
            let filter = tag_filter.trim().to_lowercase();
            presets
                .iter()
                .filter(|p| filter.is_empty() || preset_has_tag(p, &filter))
                .map(|p| {
                    let mut map = rhai::Map::new();
                    map.insert(
                        "id".into(),
                        rhai::Dynamic::from(
                            p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        ),
                    );
                    map.insert(
                        "name".into(),
                        rhai::Dynamic::from(
                            p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        ),
                    );
                    map.insert(
                        "description".into(),
                        rhai::Dynamic::from(
                            p.get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        ),
                    );
                    let tags: Vec<rhai::Dynamic> = p
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|t| t.as_str())
                                .map(|s| rhai::Dynamic::from(s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    map.insert("tags".into(), rhai::Dynamic::from(tags));
                    rhai::Dynamic::from(map)
                })
                .collect()
        },
    );

    // apply_mobile_preset(mobile_id, preset_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "apply_mobile_preset",
        move |mobile_id: String, preset_id: String| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let presets = load_presets();
            let preset = match presets
                .iter()
                .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(&preset_id))
            {
                Some(p) => p,
                None => return false,
            };

            apply_preset_to_mobile(&mut mobile, preset);

            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );
}

/// Apply a preset's bundle of fields to an existing mobile in-place.
/// Flag setters are additive (only the flags listed in the preset change).
/// Stat overrides only fire when the preset includes that field. on_hit_effects
/// are appended (deduped by the `effect` string).
pub fn apply_preset_to_mobile(mobile: &mut crate::types::MobileData, preset: &serde_json::Value) {
    if let Some(flags) = preset.get("flags").and_then(|v| v.as_object()) {
        for (k, v) in flags {
            let value = v.as_bool().unwrap_or(false);
            apply_flag(mobile, k, value);
        }
    }

    if let Some(n) = preset.get("level").and_then(|v| v.as_i64()) {
        mobile.level = n as i32;
    }
    if let Some(n) = preset.get("max_hp").and_then(|v| v.as_i64()) {
        mobile.max_hp = n as i32;
        mobile.current_hp = n as i32;
    }
    if let Some(n) = preset.get("max_stamina").and_then(|v| v.as_i64()) {
        mobile.max_stamina = n as i32;
        mobile.current_stamina = n as i32;
    }
    if let Some(n) = preset.get("armor_class").and_then(|v| v.as_i64()) {
        mobile.armor_class = n as i32;
    }
    if let Some(s) = preset.get("damage_dice").and_then(|v| v.as_str()) {
        mobile.damage_dice = s.to_string();
    }
    if let Some(s) = preset.get("creature_type").and_then(|v| v.as_str()) {
        if let Some(ct) = crate::types::CreatureType::from_str(s) {
            mobile.creature_type = ct;
        }
    }
    if let Some(n) = preset.get("perception").and_then(|v| v.as_i64()) {
        mobile.perception = n as i32;
    }
    if let Some(s) = preset.get("faction").and_then(|v| v.as_str()) {
        mobile.faction = if s.is_empty() { None } else { Some(s.to_string()) };
    }

    if let Some(arr) = preset.get("on_hit_effects").and_then(|v| v.as_array()) {
        for entry in arr {
            let effect = match entry.get("effect").and_then(|v| v.as_str()) {
                Some(e) if !e.is_empty() => e.to_string(),
                _ => continue,
            };
            // Dedupe by effect name — preset replaces an existing matching entry.
            mobile.on_hit_effects.retain(|e| e.effect != effect);
            mobile.on_hit_effects.push(crate::types::OnHitEffect {
                effect,
                chance: entry.get("chance").and_then(|v| v.as_i64()).unwrap_or(100) as i32,
                magnitude: entry.get("magnitude").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                duration: entry.get("duration").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            });
        }
    }
}

/// Mirror of the flag-name map in `set_mobile_flag`. Kept in sync deliberately —
/// preset JSON uses the same lowercase names.
fn apply_flag(mobile: &mut crate::types::MobileData, name: &str, value: bool) {
    match name.to_lowercase().as_str() {
        "aggressive" => mobile.flags.aggressive = value,
        "sentinel" => mobile.flags.sentinel = value,
        "scavenger" => mobile.flags.scavenger = value,
        "shopkeeper" => mobile.flags.shopkeeper = value,
        "no_attack" | "noattack" => mobile.flags.no_attack = value,
        "healer" => mobile.flags.healer = value,
        "leasing_agent" | "leasingagent" => mobile.flags.leasing_agent = value,
        "cowardly" => mobile.flags.cowardly = value,
        "can_open_doors" | "canopendoors" => mobile.flags.can_open_doors = value,
        "guard" => mobile.flags.guard = value,
        "helper" => mobile.flags.helper = value,
        "thief" => mobile.flags.thief = value,
        "cant_swim" | "cantswim" => mobile.flags.cant_swim = value,
        "poisonous" => mobile.flags.poisonous = value,
        "fiery" => mobile.flags.fiery = value,
        "chilling" => mobile.flags.chilling = value,
        "corrosive" => mobile.flags.corrosive = value,
        "shocking" => mobile.flags.shocking = value,
        "unique" => mobile.flags.unique = value,
        "stay_zone" | "stayzone" => mobile.flags.stay_zone = value,
        "aware" => mobile.flags.aware = value,
        "memory" => mobile.flags.memory = value,
        "no_sleep" | "nosleep" => mobile.flags.no_sleep = value,
        "no_blind" | "noblind" => mobile.flags.no_blind = value,
        "no_bash" | "nobash" => mobile.flags.no_bash = value,
        "no_summon" | "nosummon" => mobile.flags.no_summon = value,
        "no_charm" | "nocharm" => mobile.flags.no_charm = value,
        "hostile_on_steal" | "hostileonsteal" => mobile.flags.hostile_on_steal = value,
        "tameable" => mobile.flags.tameable = value,
        "undead" => mobile.flags.undead = value,
        "vampire" => mobile.flags.vampire = value,
        "holy_vulnerable" | "holyvulnerable" => mobile.flags.holy_vulnerable = value,
        _ => {}
    }
}

/// Read the preset registry from disk and find one by id. Returns None if the
/// file is missing/malformed or no preset matches. Used by callers that want
/// the data without going through Rhai (e.g. integration tests, future API).
pub fn find_preset_by_id(preset_id: &str) -> Option<serde_json::Value> {
    load_presets()
        .into_iter()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(preset_id))
}

/// Read every preset, optionally filtered by tag. Empty / whitespace `tag`
/// returns all presets. Public for HTTP API consumption (`mcp-server` layers
/// on top of this through the API).
pub fn list_presets(tag: &str) -> Vec<serde_json::Value> {
    let trimmed = tag.trim().to_lowercase();
    load_presets()
        .into_iter()
        .filter(|p| trimmed.is_empty() || preset_has_tag(p, &trimmed))
        .collect()
}

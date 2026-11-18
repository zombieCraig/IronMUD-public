//! Builder configuration of a spell-granting god (`DeitySpellcraft`).
//!
//! [`edit`] is the one place a field changes, shared by `medit <god> deity
//! spellcraft ...`; [`validate`] checks a whole config (the API path).
//! Values are stored in canonical form so later stages compare like with like.

use crate::types::{DamageType, DeitySpellcraft, EffectType, MarkTemplate, PriceStyle, TrialRule};

use super::power::ALLOWED_PLAYER_TYPES;

/// Targets a player spell may have.
pub const ALLOWED_TARGETS: &[&str] = &["self", "self_or_friendly", "enemy"];

fn canonical_type(v: &str) -> Option<String> {
    let v = v.to_ascii_lowercase();
    ALLOWED_PLAYER_TYPES.contains(&v.as_str()).then_some(v)
}

fn canonical_target(v: &str) -> Option<String> {
    let v = v.to_ascii_lowercase();
    ALLOWED_TARGETS.contains(&v.as_str()).then_some(v)
}

fn canonical_element(v: &str) -> Option<String> {
    DamageType::from_str(v).map(|d| d.to_display_string().to_string())
}

fn canonical_effect(v: &str) -> Option<String> {
    EffectType::from_str(v)
        .filter(|e| *e != EffectType::None)
        .map(|e| e.to_display_string().to_string())
}

/// Maps a raw value to its stored form, or `None` if it isn't valid.
type Canonicaliser = fn(&str) -> Option<String>;

/// The list fields: `(canonicaliser, what a bad value should be)`.
fn list_kind(field: &str) -> Option<(Canonicaliser, String)> {
    match field {
        "types" => Some((canonical_type, ALLOWED_PLAYER_TYPES.join(", "))),
        "targets" => Some((canonical_target, ALLOWED_TARGETS.join(", "))),
        "elements" => Some((canonical_element, "a damage type (see `lookup damage`)".to_string())),
        "effects" => Some((canonical_effect, "an effect type (see `lookup effect`)".to_string())),
        _ => None,
    }
}

fn list_mut<'a>(cfg: &'a mut DeitySpellcraft, field: &str) -> &'a mut Vec<String> {
    match field {
        "types" => &mut cfg.allowed_types,
        "targets" => &mut cfg.allowed_targets,
        "elements" => &mut cfg.allowed_elements,
        _ => &mut cfg.allowed_effects,
    }
}

fn int_in(v: &str, lo: i64, hi: i64, what: &str) -> Result<i64, String> {
    match v.parse::<i64>() {
        Ok(n) if (lo..=hi).contains(&n) => Ok(n),
        _ => Err(format!("{what} must be a number from {lo} to {hi}.")),
    }
}

/// `"default"`/`"none"` clears an override.
fn override_in(v: &str, lo: i64, hi: i64, what: &str) -> Result<Option<i64>, String> {
    if v.eq_ignore_ascii_case("default") || v.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    int_in(v, lo, hi, what).map(Some)
}

/// Apply one builder edit. `field` is a list name (`types`, `elements`,
/// `effects`, `targets`) with `op` add/remove/clear, or a scalar (`cap`,
/// `cooldown`, `favor`, `tier`, `cost`, `unlock`, `priest`) with `op` set.
pub fn edit(cfg: &mut DeitySpellcraft, field: &str, op: &str, value: &str) -> Result<(), String> {
    let field = field.to_ascii_lowercase();
    let op = op.to_ascii_lowercase();
    if let Some((canon, expect)) = list_kind(&field) {
        let list = list_mut(cfg, &field);
        match op.as_str() {
            "clear" => {
                list.clear();
                return Ok(());
            }
            "add" | "remove" => {
                let Some(v) = canon(value) else {
                    return Err(format!("'{value}' is not valid here; expected {expect}."));
                };
                if op == "add" {
                    if !list.contains(&v) {
                        list.push(v);
                    }
                } else {
                    list.retain(|x| *x != v);
                }
                return Ok(());
            }
            _ => return Err(format!("Use: {field} add|remove <value> | {field} clear")),
        }
    }
    if field == "trial" {
        return edit_trials(cfg, &op, value);
    }
    if field == "mark" {
        return edit_marks(cfg, &op, value);
    }
    if op != "set" {
        return Err(format!("Unknown spellcraft setting '{field}'."));
    }
    match field.as_str() {
        "cap" => cfg.canon_cap = int_in(value, 1, 20, "The canon cap")? as u32,
        "cooldown" => cfg.petition_cooldown_days = override_in(value, 0, 365, "The petition cooldown")?,
        "favor" => cfg.min_favor = override_in(value, 0, 100_000, "Minimum favor")?,
        "tier" => cfg.min_power_tier = override_in(value, 1, 5, "The minimum power tier")?,
        "cost" => cfg.favor_cost = int_in(value, 0, 100_000, "The favor cost")?,
        "unlock" => {
            cfg.unlock_quest_id = if value.is_empty() || value.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(value.to_string())
            }
        }
        "price" => {
            cfg.price_style = PriceStyle::parse(value).ok_or_else(|| {
                let all: Vec<&str> = PriceStyle::ALL.iter().map(|p| p.as_str()).collect();
                format!("Price must be one of: {}.", all.join(", "))
            })?
        }
        "priest" => {
            cfg.priest_teaches = match value.to_ascii_lowercase().as_str() {
                "on" | "true" | "yes" => true,
                "off" | "false" | "no" => false,
                _ => return Err("Use: priest set on|off".to_string()),
            }
        }
        _ => return Err(format!("Unknown spellcraft setting '{field}'.")),
    }
    Ok(())
}

/// `trial add <quest> [type=..] [element=..] [effect=..] [power=N]`,
/// `trial remove <n>` (1-based), `trial clear`.
fn edit_trials(cfg: &mut DeitySpellcraft, op: &str, value: &str) -> Result<(), String> {
    const USAGE: &str = "Use: trial add <quest_id> [type=<t>] [element=<e>] [effect=<e>] [power=<n>] [restore=yes] | trial remove <n> | trial clear";
    match op {
        "clear" => cfg.trial_rules.clear(),
        "remove" => {
            let n = value.trim().parse::<usize>().map_err(|_| USAGE.to_string())?;
            if n == 0 || n > cfg.trial_rules.len() {
                return Err(format!("There is no trial {n}."));
            }
            cfg.trial_rules.remove(n - 1);
        }
        "add" => {
            let mut words = value.split_whitespace();
            let quest_id = words.next().ok_or_else(|| USAGE.to_string())?.to_string();
            let mut rule = TrialRule {
                quest_id,
                ..TrialRule::default()
            };
            for w in words {
                let (k, v) = w.split_once('=').ok_or_else(|| USAGE.to_string())?;
                set_trial_key(&mut rule, k, v)?;
            }
            cfg.trial_rules.push(rule);
        }
        _ => return Err(USAGE.to_string()),
    }
    Ok(())
}

/// `mark add <body part> <effect[:mag]>[,<effect[:mag]>...] <description>`,
/// `mark remove <n>` (1-based), `mark clear`.
fn edit_marks(cfg: &mut DeitySpellcraft, op: &str, value: &str) -> Result<(), String> {
    const USAGE: &str =
        "Use: mark add <body part> <effect[:mag]>[,<effect[:mag]>] <description> | mark remove <n> | mark clear";
    match op {
        "clear" => cfg.mark_pool.clear(),
        "remove" => {
            let n = value.trim().parse::<usize>().map_err(|_| USAGE.to_string())?;
            if n == 0 || n > cfg.mark_pool.len() {
                return Err(format!("There is no mark {n}."));
            }
            cfg.mark_pool.remove(n - 1);
        }
        "add" => {
            let mut words = value.split_whitespace();
            let (Some(body_part), Some(specs)) = (words.next(), words.next()) else {
                return Err(USAGE.to_string());
            };
            let short_desc = words.collect::<Vec<_>>().join(" ");
            if short_desc.is_empty() {
                return Err(USAGE.to_string());
            }
            let affects = specs
                .split(',')
                .filter(|x| !x.is_empty() && !x.eq_ignore_ascii_case("none"))
                .map(super::price::parse_affect)
                .collect::<Result<Vec<_>, _>>()?;
            cfg.mark_pool.push(MarkTemplate {
                short_desc: super::text::sanitize_flavor(&short_desc),
                body_part: body_part.to_ascii_lowercase(),
                affects,
            });
        }
        _ => return Err(USAGE.to_string()),
    }
    Ok(())
}

/// One line for a mark template: `a sun-brand (hand): regen_penalty +15`.
pub fn describe_mark(t: &MarkTemplate) -> String {
    let affects: Vec<String> = t
        .affects
        .iter()
        .map(|a| format!("{} {:+}", a.effect_type.to_display_string(), a.magnitude))
        .collect();
    let what = if affects.is_empty() {
        "no affects".to_string()
    } else {
        affects.join(", ")
    };
    format!("{} ({}): {}", t.short_desc, t.body_part, what)
}

fn set_trial_key(rule: &mut TrialRule, k: &str, v: &str) -> Result<(), String> {
    let bad = |what: &str| format!("'{v}' is not a valid {what}.");
    match k.to_ascii_lowercase().as_str() {
        "type" => rule.when_type = Some(canonical_type(v).ok_or_else(|| bad("spell type"))?),
        "element" => rule.when_element = Some(canonical_element(v).ok_or_else(|| bad("element"))?),
        "effect" => rule.when_effect = Some(canonical_effect(v).ok_or_else(|| bad("effect"))?),
        "power" => rule.min_power = v.parse::<u32>().map_err(|_| bad("power"))?,
        "restore" | "restoration" => {
            rule.restoration = match v.to_ascii_lowercase().as_str() {
                "yes" | "on" | "true" => true,
                "no" | "off" | "false" => false,
                _ => return Err(bad("yes/no")),
            }
        }
        _ => return Err(format!("Unknown trial condition '{k}'.")),
    }
    Ok(())
}

/// One line for a trial rule: `rite_of_flame (damage, fire, power 120+)`.
pub fn describe_trial(rule: &TrialRule) -> String {
    let mut when: Vec<String> = [&rule.when_type, &rule.when_element, &rule.when_effect]
        .iter()
        .filter_map(|w| (*w).clone())
        .collect();
    if rule.min_power > 0 {
        when.push(format!("power {}+", rule.min_power));
    }
    if rule.restoration {
        when.push("restoration".to_string());
    }
    if when.is_empty() {
        format!("{} (any spell)", rule.quest_id)
    } else {
        format!("{} ({})", rule.quest_id, when.join(", "))
    }
}

/// Check (and canonicalise) a whole config, as the API receives it.
pub fn validate(cfg: &mut DeitySpellcraft) -> Result<(), String> {
    for field in ["types", "targets", "elements", "effects"] {
        let (canon, expect) = list_kind(field).expect("list field");
        let list = list_mut(cfg, field);
        let mut out: Vec<String> = Vec::new();
        for v in list.iter() {
            let c = canon(v).ok_or_else(|| format!("spellcraft.{field}: '{v}' is not valid; expected {expect}"))?;
            if !out.contains(&c) {
                out.push(c);
            }
        }
        *list = out;
    }
    for rule in &mut cfg.trial_rules {
        if rule.quest_id.trim().is_empty() {
            return Err("spellcraft.trial_rules: quest_id is required".to_string());
        }
        let canon = |v: &Option<String>, f: fn(&str) -> Option<String>, what: &str| match v {
            None => Ok(None),
            Some(x) => f(x)
                .map(Some)
                .ok_or_else(|| format!("spellcraft.trial_rules: '{x}' is not a valid {what}")),
        };
        rule.when_type = canon(&rule.when_type, canonical_type, "spell type")?;
        rule.when_element = canon(&rule.when_element, canonical_element, "element")?;
        rule.when_effect = canon(&rule.when_effect, canonical_effect, "effect")?;
    }
    if !(1..=20).contains(&cfg.canon_cap) {
        return Err("spellcraft.canon_cap must be 1-20".to_string());
    }
    if cfg.min_power_tier.is_some_and(|t| !(1..=5).contains(&t)) {
        return Err("spellcraft.min_power_tier must be 1-5".to_string());
    }
    if cfg.favor_cost < 0 || cfg.min_favor.is_some_and(|f| f < 0) || cfg.petition_cooldown_days.is_some_and(|d| d < 0) {
        return Err("spellcraft numbers must not be negative".to_string());
    }
    for t in &mut cfg.mark_pool {
        t.short_desc = super::text::sanitize_flavor(&t.short_desc);
        t.body_part = super::text::sanitize_flavor(&t.body_part).to_ascii_lowercase();
        if t.short_desc.is_empty() {
            return Err("spellcraft.mark_pool: short_desc is required".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_canonicalise_and_reject_junk() {
        let mut c = DeitySpellcraft::default();
        edit(&mut c, "types", "add", "Damage").unwrap();
        edit(&mut c, "types", "add", "damage").unwrap();
        assert_eq!(c.allowed_types, vec!["damage"]);
        assert!(
            edit(&mut c, "types", "add", "utility").is_err(),
            "utility is builder-only"
        );
        edit(&mut c, "effects", "add", "sanctuary").unwrap();
        assert_eq!(c.allowed_effects, vec!["damage_reduction"], "aliases store canonically");
        assert!(edit(&mut c, "effects", "add", "none").is_err());
        edit(&mut c, "elements", "add", "fire").unwrap();
        assert!(edit(&mut c, "elements", "add", "jelly").is_err());
        edit(&mut c, "targets", "add", "enemy").unwrap();
        assert!(edit(&mut c, "targets", "add", "room").is_err());
        edit(&mut c, "types", "remove", "damage").unwrap();
        assert!(c.allowed_types.is_empty());
        edit(&mut c, "elements", "clear", "").unwrap();
        assert!(c.allowed_elements.is_empty());
    }

    #[test]
    fn scalars_and_overrides() {
        let mut c = DeitySpellcraft::default();
        edit(&mut c, "cap", "set", "3").unwrap();
        assert_eq!(c.canon_cap, 3);
        assert!(edit(&mut c, "cap", "set", "0").is_err());
        edit(&mut c, "favor", "set", "750").unwrap();
        assert_eq!(c.min_favor, Some(750));
        edit(&mut c, "favor", "set", "default").unwrap();
        assert_eq!(c.min_favor, None);
        edit(&mut c, "tier", "set", "4").unwrap();
        assert!(edit(&mut c, "tier", "set", "6").is_err());
        edit(&mut c, "unlock", "set", "rite_open_hand").unwrap();
        assert_eq!(c.unlock_quest_id.as_deref(), Some("rite_open_hand"));
        edit(&mut c, "unlock", "set", "none").unwrap();
        assert_eq!(c.unlock_quest_id, None);
        edit(&mut c, "priest", "set", "off").unwrap();
        assert!(!c.priest_teaches);
        assert!(edit(&mut c, "bogus", "set", "1").is_err());
    }

    #[test]
    fn price_and_trials() {
        let mut c = DeitySpellcraft::default();
        edit(&mut c, "price", "set", "Weariness").unwrap();
        assert_eq!(c.price_style, PriceStyle::Weariness);
        assert!(edit(&mut c, "price", "set", "blood").is_err());
        edit(&mut c, "trial", "add", "rite_any").unwrap();
        edit(&mut c, "trial", "add", "rite_fire type=Damage element=fire power=120").unwrap();
        assert_eq!(c.trial_rules[1].when_type.as_deref(), Some("damage"));
        assert_eq!(c.trial_rules[1].min_power, 120);
        assert_eq!(
            describe_trial(&c.trial_rules[1]),
            "rite_fire (damage, fire, power 120+)"
        );
        assert_eq!(describe_trial(&c.trial_rules[0]), "rite_any (any spell)");
        assert!(edit(&mut c, "trial", "add", "x element=jelly").is_err());
        assert!(edit(&mut c, "trial", "add", "").is_err());
        edit(&mut c, "trial", "remove", "1").unwrap();
        assert_eq!(c.trial_rules[0].quest_id, "rite_fire");
        assert!(edit(&mut c, "trial", "remove", "5").is_err());
        edit(&mut c, "trial", "clear", "").unwrap();
        assert!(c.trial_rules.is_empty());
    }

    #[test]
    fn validate_canonicalises_api_input() {
        let mut c = DeitySpellcraft {
            allowed_types: vec!["HEAL".into(), "heal".into()],
            allowed_effects: vec!["stoneskin".into()],
            ..DeitySpellcraft::default()
        };
        validate(&mut c).unwrap();
        assert_eq!(c.allowed_types, vec!["heal"]);
        assert_eq!(c.allowed_effects, vec!["damage_reduction"]);
        c.allowed_targets = vec!["world".into()];
        assert!(validate(&mut c).is_err());
    }
}

//! Builder-defined custom skills.
//!
//! Builders publish a `CustomSkillDefinition` via `lookup skill publish` to
//! advertise a named integer attribute (e.g. `dancing_queen`) that DG scripts
//! and items can read/write. The registry itself stores only metadata —
//! per-entity values live in `CharacterData.custom_skills` and
//! `MobileData.custom_skills`, and equipment can stamp `EffectType::CustomSkillBoost`
//! buffs that aggregate into `get_effective_custom_skill`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSkillDefinition {
    /// Canonical key. Must match `^[a-z][a-z0-9_]{1,31}$` and not collide
    /// with any hardcoded entry in `crate::script::lookup::KNOWN_SKILLS`.
    pub key: String,
    /// One- or two-sentence description of what the skill represents.
    pub description: String,
    /// Character name of the builder who published it. Used to gate
    /// `unpublish` (non-admin builders can only remove their own).
    pub author: String,
    /// Unix seconds at publish time.
    pub created_at: i64,
}

/// Key validation. Returns `true` when the key is a syntactically valid
/// custom skill identifier (does not check registry membership or collisions).
pub fn is_valid_custom_skill_key(key: &str) -> bool {
    let len = key.len();
    if !(2..=32).contains(&len) {
        return false;
    }
    let mut chars = key.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_keys() {
        assert!(is_valid_custom_skill_key("dancing_queen"));
        assert!(is_valid_custom_skill_key("a1"));
        assert!(is_valid_custom_skill_key("lockpicking"));
    }

    #[test]
    fn invalid_keys() {
        assert!(!is_valid_custom_skill_key(""));
        assert!(!is_valid_custom_skill_key("a"));                  // too short
        assert!(!is_valid_custom_skill_key("1abc"));               // starts with digit
        assert!(!is_valid_custom_skill_key("Dancing"));            // uppercase
        assert!(!is_valid_custom_skill_key("dance queen"));        // space
        assert!(!is_valid_custom_skill_key("dance!"));             // punct
        assert!(!is_valid_custom_skill_key(&"a".repeat(33)));      // too long
    }
}

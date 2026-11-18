//! Player-written spell text: names, descriptions and cast flavour.
//!
//! There is no profanity filter in the codebase to reuse; names follow the
//! same shape rules as character names (`is_valid_username`), narrowed to
//! letters, spaces and apostrophes. Free text is stripped of control
//! characters (so no ANSI injection), MXP-escaped and length-capped.

use std::collections::HashMap;

use crate::script::utilities::escape_mxp;
use crate::types::SpellDefinition;

pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 32;
pub const DESCRIPTION_MAX: usize = 240;
pub const FLAVOR_MAX: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Length,
    Characters,
    /// Same as, or a prefix either way of, an existing spell's id or name.
    /// `cast` resolves spells by prefix, so such a name would shadow or be
    /// shadowed. Carries the clashing spell's name.
    Collides(String),
}

impl NameError {
    pub fn message(&self) -> String {
        match self {
            Self::Length => format!("A spell name must be {NAME_MIN}-{NAME_MAX} characters."),
            Self::Characters => {
                "A spell name may use only letters, single spaces and apostrophes, starting with a letter.".to_string()
            }
            Self::Collides(other) => format!("That name is too close to the existing spell '{other}'."),
        }
    }
}

/// `"Ember  Lance"` → `"ember lance"`: lowercase, single spaces.
fn normalize(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn prefix_either_way(a: &str, b: &str) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Check a proposed spell name. `exclude_id` skips the spell being renamed.
pub fn validate_spell_name(
    name: &str,
    spells: &HashMap<String, SpellDefinition>,
    exclude_id: Option<&str>,
) -> Result<(), NameError> {
    let n = name.chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&n) {
        return Err(NameError::Length);
    }
    let first_ok = name.chars().next().is_some_and(|c| c.is_alphabetic());
    let chars_ok = name.chars().all(|c| c.is_alphabetic() || c == ' ' || c == '\'');
    if !first_ok || !chars_ok || name.contains("  ") || name.ends_with(' ') {
        return Err(NameError::Characters);
    }

    let wanted = normalize(name);
    let wanted_id = wanted.replace(' ', "_");
    for (id, def) in spells {
        if Some(id.as_str()) == exclude_id {
            continue;
        }
        let other_name = normalize(&def.name);
        let other_id = id.to_lowercase();
        // `cx_` ids are never typed by players; their names are what clash.
        let id_clash = !other_id.starts_with(crate::types::CODEX_ID_PREFIX) && prefix_either_way(&wanted_id, &other_id);
        if prefix_either_way(&wanted, &other_name) || id_clash {
            return Err(NameError::Collides(def.name.clone()));
        }
    }
    Ok(())
}

/// The codex id for a spell name: `"Ember Lance"` → `"cx_ember_lance"`.
pub fn codex_id_for(name: &str) -> String {
    let slug: String = normalize(name)
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('_'),
            '\'' => None,
            c => Some(c),
        })
        .collect();
    format!("{}{}", crate::types::CODEX_ID_PREFIX, slug)
}

/// Strip control characters (ANSI escapes included), collapse whitespace,
/// cap at `max` characters, then MXP-escape.
fn clean(text: &str, max: usize) -> String {
    let no_controls: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let collapsed = no_controls.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(max).collect();
    escape_mxp(&capped)
}

/// Player-written cast line, safe to send to clients.
pub fn sanitize_flavor(text: &str) -> String {
    clean(text, FLAVOR_MAX)
}

/// Player-written spell description, safe to send to clients.
pub fn sanitize_description(text: &str) -> String {
    clean(text, DESCRIPTION_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spells() -> HashMap<String, SpellDefinition> {
        let mut m = HashMap::new();
        for (id, name) in [
            ("firebolt", "Firebolt"),
            ("magic_missile", "Magic Missile"),
            ("cx_ember_lance", "Ember Lance"),
        ] {
            let def: SpellDefinition =
                serde_json::from_value(serde_json::json!({"id": id, "name": name, "description": ""})).unwrap();
            m.insert(id.to_string(), def);
        }
        m
    }

    #[test]
    fn accepts_a_fresh_name() {
        assert_eq!(validate_spell_name("Sun's Wrath", &spells(), None), Ok(()));
    }

    #[test]
    fn rejects_bad_shapes() {
        let s = spells();
        assert_eq!(validate_spell_name("Ox", &s, None), Err(NameError::Length));
        assert_eq!(validate_spell_name(&"a".repeat(33), &s, None), Err(NameError::Length));
        assert_eq!(validate_spell_name("Bolt 2", &s, None), Err(NameError::Characters));
        assert_eq!(validate_spell_name("'Bolt", &s, None), Err(NameError::Characters));
        assert_eq!(validate_spell_name("Big  Bolt", &s, None), Err(NameError::Characters));
        assert_eq!(validate_spell_name("Bolt<b>", &s, None), Err(NameError::Characters));
    }

    #[test]
    fn rejects_exact_and_prefix_collisions_both_ways() {
        let s = spells();
        // Exact name, case-insensitive.
        assert!(matches!(
            validate_spell_name("firebolt", &s, None),
            Err(NameError::Collides(_))
        ));
        // New name is a prefix of an existing name.
        assert!(matches!(
            validate_spell_name("Fire", &s, None),
            Err(NameError::Collides(_))
        ));
        // Existing name is a prefix of the new name.
        assert!(matches!(
            validate_spell_name("Magic Missile Storm", &s, None),
            Err(NameError::Collides(_))
        ));
        // Against a codex spell's name.
        assert!(matches!(
            validate_spell_name("Ember", &s, None),
            Err(NameError::Collides(_))
        ));
        // Against an id typed with underscores ("magic m" resolves to magic_missile).
        assert!(matches!(
            validate_spell_name("Magic", &s, None),
            Err(NameError::Collides(_))
        ));
    }

    #[test]
    fn rename_may_keep_its_own_name() {
        assert_eq!(
            validate_spell_name("Ember Lance", &spells(), Some("cx_ember_lance")),
            Ok(())
        );
    }

    #[test]
    fn codex_ids_are_slugs() {
        assert_eq!(codex_id_for("Sun's  Wrath"), "cx_suns_wrath");
    }

    #[test]
    fn flavor_is_escaped_stripped_and_capped() {
        let out = sanitize_flavor("<send href='x'>click</send> \x1b[31mred\x1b[0m");
        assert!(!out.contains('<') && !out.contains('>'), "{out}");
        assert!(!out.contains('\x1b'), "{out}");
        assert!(out.contains("&lt;send"));
        let long = sanitize_flavor(&"a".repeat(500));
        assert_eq!(long.chars().count(), FLAVOR_MAX);
    }
}

//! Scribing (§9, S9): `craft scroll <spell>` inks a codex spell onto a
//! blank scroll, so spells spread player to player through the item
//! economy rather than a `teach` verb.
//!
//! The blank is inscribed *in place*: one CAS write claims it (only an
//! un-inked blank in the scribe's pack qualifies) and turns it into the
//! scroll, so builders need only a blank-scroll prototype (category
//! [`BLANK_SCROLL_CATEGORY`]), never a per-spell one.

use super::{Settings, price};
use crate::db::Db;
use crate::script::achievements::apply_to_character;
use crate::types::{CanonStatus, CharacterData, ItemData, ItemLocation, SpellOrigin};
use crate::{SharedConnections, SharedState};

/// Item category a blank scroll carries.
pub const BLANK_SCROLL_CATEGORY: &str = "blank_scroll";
/// Category an inscribed scroll carries instead.
pub const INSCRIBED_CATEGORY: &str = "scroll:inscribed";
/// Spellcraft XP per scroll.
pub const XP_PER_SCROLL: i32 = 25;

/// The codex spell `query` names among those `ch` knows: exact id, exact
/// name, then a unique name prefix.
fn known_codex_spell(state: &SharedState, ch: &CharacterData, query: &str) -> Option<(String, String, i32)> {
    let q = query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    if q.is_empty() {
        return None;
    }
    let world = state.lock();
    let known: Vec<(&String, &crate::types::SpellDefinition)> = ch
        .learned_spells
        .iter()
        .filter_map(|id| world.spell_definitions.get(id).map(|d| (id, d)))
        .filter(|(_, d)| d.codex)
        .collect();
    let pick = known
        .iter()
        .find(|(id, d)| id.as_str() == q || d.name.to_lowercase() == q)
        .or_else(|| {
            let hits: Vec<_> = known
                .iter()
                .filter(|(_, d)| d.name.to_lowercase().starts_with(&q))
                .collect();
            if hits.len() == 1 { Some(hits[0]) } else { None }
        })?;
    Some((pick.0.clone(), pick.1.name.clone(), pick.1.mana_cost))
}

fn is_blank(item: &ItemData) -> bool {
    item.teaches_spell.is_none()
        && item
            .categories
            .iter()
            .any(|c| c.eq_ignore_ascii_case(BLANK_SCROLL_CATEGORY))
}

/// Turn a blank into the scroll of `spell_name` (pure).
pub fn inscribe(item: &mut ItemData, spell_id: &str, spell_name: &str) {
    item.teaches_spell = Some(spell_id.to_string());
    item.teaches_recipe = None;
    item.note_content = None;
    item.name = "inscribed scroll".to_string();
    item.short_desc = format!("a scroll inked with {spell_name}");
    item.long_desc = format!("A scroll inked with the words of {spell_name} lies here.");
    let mut keywords = vec!["scroll".to_string(), "inscribed".to_string()];
    for w in spell_name.split_whitespace() {
        let w = w.to_lowercase();
        if !keywords.contains(&w) {
            keywords.push(w);
        }
    }
    item.keywords = keywords;
    item.categories
        .retain(|c| !c.eq_ignore_ascii_case(BLANK_SCROLL_CATEGORY));
    item.categories.push(INSCRIBED_CATEGORY.to_string());
}

/// Claim one blank in `owner`'s pack and inscribe it. Exactly once: only
/// an un-inked blank still in the pack is taken. Returns the scroll as it
/// was before (to put back) on success.
fn claim_blank(db: &Db, owner: &str, spell_id: &str, spell_name: &str) -> Option<ItemData> {
    let mut blanks: Vec<ItemData> = db
        .get_items_in_inventory(owner)
        .unwrap_or_default()
        .into_iter()
        .filter(is_blank)
        .collect();
    blanks.sort_by_key(|i| i.quality);
    for blank in blanks {
        let mut before = None;
        let _ = db.update_item(&blank.id, |i| {
            before = None;
            let mine = matches!(&i.location, ItemLocation::Inventory(o) if o.eq_ignore_ascii_case(owner));
            if mine && is_blank(i) {
                before = Some(i.clone());
                inscribe(i, spell_id, spell_name);
            }
        });
        if before.is_some() {
            return before;
        }
    }
    None
}

/// `craft scroll <spell>`. Returns what the scribe sees.
pub fn scribe(db: &Db, connections: &SharedConnections, state: &SharedState, name: &str, query: &str) -> String {
    let settings = Settings::load(db);
    if !settings.enabled {
        return "You don't know how to do that.\n".to_string();
    }
    let Some(ch) = apply_to_character(db, connections, name, |_| false) else {
        return "You must be logged in to do that.\n".to_string();
    };
    if query.trim().is_empty() {
        return "Inscribe which spell? (craft scroll <spell>)\n".to_string();
    }
    let Some((spell_id, spell_name, mana_cost)) = known_codex_spell(state, &ch, query) else {
        return format!("You know no spell of the gods called '{}'.\n", query.trim());
    };
    let Ok(Some(entry)) = db.get_codex(&spell_id) else {
        return format!("You know no spell of the gods called '{}'.\n", query.trim());
    };
    if entry.meta.vetoed {
        return format!("The words of {spell_name} will not stay on the page.\n");
    }
    let skill = ch.skills.get(super::SPELLCRAFT_SKILL).map(|s| s.level).unwrap_or(0) as i64;
    if skill < settings.scribe_min_spellcraft {
        return format!(
            "You need spellcraft {} to set a spell to parchment. (You have {skill})\n",
            settings.scribe_min_spellcraft
        );
    }
    // The author's monopoly: a fresh Canon spell is theirs to spread.
    let today = crate::script::worship::current_absolute_day(db);
    if entry.meta.status == CanonStatus::Canon
        && entry.meta.origin == SpellOrigin::Authored
        && let Some(author) = entry.meta.author.as_deref()
        && !author.eq_ignore_ascii_case(name)
    {
        let open_day = entry.meta.sealed_day + settings.scribe_monopoly_days;
        if today < open_day {
            return format!(
                "{spell_name} is still {author}'s to write. ({} more day{})\n",
                open_day - today,
                if open_day - today == 1 { "" } else { "s" }
            );
        }
    }
    let cost = mana_cost.max(1);
    if ch.mana < cost {
        return format!("You need {cost} mana to hold the words while you write.\n");
    }

    // Claim first, then pay; on a failed payment put the blank back.
    let Some(before) = claim_blank(db, name, &spell_id, &spell_name) else {
        return "You need a blank scroll to write on.\n".to_string();
    };
    let mut paid = false;
    apply_to_character(db, connections, name, |c| {
        paid = c.mana >= cost;
        if paid {
            c.mana -= cost;
        }
        paid
    });
    if !paid {
        let _ = db.save_item_data(before);
        return format!("You need {cost} mana to hold the words while you write.\n");
    }

    crate::script::achievements::notify_counter_core(db, connections, state, name, super::fame::SCRIBED_COUNTER, 1);
    crate::progress::award_xp(
        db,
        connections,
        state,
        name,
        super::SPELLCRAFT_SKILL,
        XP_PER_SCROLL,
        crate::progress::XpSource::Craft,
    );
    crate::session::broadcast_to_room(
        connections,
        ch.current_room_id,
        format!("{name} inks a scroll, murmuring as the words take.\n"),
        Some(name),
    );
    let god = price::god_name(db, &entry.meta.god_vnum);
    format!("You ink {spell_name}, a prayer of {god}, onto the scroll. ({cost} mana)\n")
}

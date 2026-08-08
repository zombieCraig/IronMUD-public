//! Corpse creation using builder pattern

use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::{ItemData, ItemFlags, ItemLocation, ItemType, LiquidType};

/// Builder for creating corpses from dead entities
pub struct CorpseBuilder {
    name: String,
    room_id: Uuid,
    gold: i64,
    is_player: bool,
    source_vnum: Option<String>,
}

impl CorpseBuilder {
    /// Create a new corpse builder for a player
    pub fn for_player(name: &str, room_id: Uuid, gold: i64) -> Self {
        Self {
            name: name.to_string(),
            room_id,
            gold,
            is_player: true,
            source_vnum: None,
        }
    }

    /// Create a new corpse builder for a mobile
    pub fn for_mobile(name: &str, room_id: Uuid, gold: i64) -> Self {
        Self {
            name: name.to_string(),
            room_id,
            gold,
            is_player: false,
            source_vnum: None,
        }
    }

    /// Record the source mob's prototype vnum so animate_dead can reanimate it.
    pub fn with_source_vnum(mut self, vnum: Option<String>) -> Self {
        self.source_vnum = vnum;
        self
    }

    /// Build the corpse ItemData
    pub fn build(self) -> ItemData {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        ItemData {
            authored_by: None,
            last_edited_by: None,
            origin: Default::default(),
            id: Uuid::new_v4(),
            name: format!("corpse of {}", self.name),
            short_desc: format!("The corpse of {} lies here.", self.name),
            long_desc: format!("The lifeless body of {} lies in a crumpled heap.", self.name),
            area_id: None,
            keywords: vec!["corpse".to_string(), "body".to_string(), self.name.to_lowercase()],
            item_type: ItemType::Container,
            categories: Vec::new(),
            teaches_recipe: None,
            teaches_spell: None,
            note_content: None,
            board_read_admin_only: false,
            board_write_admin_only: false,
            board_max_messages: None,
            donated_at: None,
            extra_descs: Vec::new(),
            wear_locations: vec![],
            armor_class: None,
            hit_bonus: 0,
            damage_bonus: 0,
            max_hp_bonus: 0,
            max_mana_bonus: 0,
            light_hours_remaining: 0,
            cast_on_use: None,
            protects: vec![],
            flags: ItemFlags {
                no_get: true,
                is_corpse: true,
                corpse_owner: self.name.clone(),
                corpse_created_at: now,
                corpse_is_player: self.is_player,
                corpse_gold: self.gold,
                corpse_source_vnum: self.source_vnum.clone(),
                ..Default::default()
            },
            weight: 100,
            value: 0,
            location: ItemLocation::Room(self.room_id),
            currently_worn_at: None,
            damage_dice_count: 0,
            damage_dice_sides: 0,
            damage_type: Default::default(),
            two_handed: false,
            weapon_skill: None,
            on_hit_effects: Vec::new(),
            // Container fields - corpses are containers
            container_contents: vec![],
            container_max_items: 1000,
            container_max_weight: 10000,
            container_closed: false,
            container_locked: false,
            container_key_vnum: None,
            weight_reduction: 0,
            // Liquid container fields
            liquid_type: LiquidType::default(),
            liquid_current: 0,
            liquid_max: 0,
            liquid_poisoned: false,
            liquid_effects: vec![],
            // Food fields
            food_nutrition: 0,
            food_poisoned: false,
            food_spoil_duration: 0,
            food_created_at: None,
            food_effects: vec![],
            food_spoilage_points: 0.0,
            preservation_level: 0,
            // Level/stats
            level_requirement: 0,
            stat_str: 0,
            stat_dex: 0,
            stat_con: 0,
            stat_int: 0,
            stat_wis: 0,
            stat_cha: 0,
            insulation: 0,
            is_prototype: false,
            vnum: None,
            world_max_count: None,
            triggers: vec![],
            vending_stock: vec![],
            vending_sell_rate: 150,
            quality: 0,
            bait_uses: 0,
            holes: 0,
            medical_tier: 0,
            medical_uses: 0,
            treats_wound_types: vec![],
            max_treatable_wound: String::new(),
            transport_link: None,
            caliber: None,
            ammo_count: 0,
            ammo_damage_bonus: 0,
            ranged_type: None,
            magazine_size: 0,
            loaded_ammo: 0,
            loaded_ammo_bonus: 0,
            loaded_ammo_vnum: None,
            fire_mode: String::new(),
            supported_fire_modes: vec![],
            noise_level: String::new(),
            ammo_effect_type: String::new(),
            ammo_effect_duration: 0,
            ammo_effect_damage: 0,
            loaded_ammo_effect_type: String::new(),
            loaded_ammo_effect_duration: 0,
            loaded_ammo_effect_damage: 0,
            attachment_slot: String::new(),
            attachment_accuracy_bonus: 0,
            attachment_noise_reduction: 0,
            attachment_magazine_bonus: 0,
            attachment_compatible_types: Vec::new(),
            plant_prototype_vnum: String::new(),
            fertilizer_duration: 0,
            treats_infestation: String::new(),
            dg_vars: std::collections::HashMap::new(),
            affects: Vec::new(),
            cyber_category: None,
            cyber_foundation: false,
            cyber_option_slots: 0,
            cyber_slot_cost: 0,
            cyber_humanity_loss: 0,
            cyber_paired: false,
            cyber_exclusive_tag: String::new(),
        }
    }
}

/// Calculate gold drop with random variance for mobiles.
///
/// Single source of truth for mob corpse gold: both the combat-tick death path
/// (`process_mobile_death`) and the script death paths (`set_corpse_gold`) route
/// through here so the drop math never diverges.
///
/// Applies a ±20% spread, rounded (not truncated) and floored at ±1 so even
/// small values like 5 actually vary (4..=6) instead of dropping a flat amount.
/// A non-positive base always yields 0.
pub fn mobile_gold_with_variance(base_gold: i64) -> i64 {
    use rand::Rng;

    if base_gold > 0 {
        let mut rng = rand::thread_rng();
        let variance = ((base_gold as f64 * 0.20).round() as i64).max(1);
        let min = (base_gold - variance).max(0);
        let max = base_gold + variance;
        let rolled = rng.gen_range(min..=max);
        tracing::debug!("gold drop: base={} -> rolled={}", base_gold, rolled);
        rolled
    } else {
        tracing::debug!("gold drop: base={} -> rolled=0 (no gold)", base_gold);
        0
    }
}

/// Render a corpse's contents as one human-readable clause, e.g.
/// `"37 gold, a chipped bone knife and 2 ration tins"`. Returns `None` when
/// there is nothing worth mentioning, so the caller can omit the line entirely
/// rather than print "it carried nothing" after every kill.
///
/// Exists so the killer learns what dropped without having to `look in corpse`
/// after every single fight — the most repeated action in the game had no
/// resolution feedback at all.
///
/// Takes the names directly rather than the ids so it stays pure and testable;
/// [`describe_corpse_contents`] does the DB reads.
pub fn format_corpse_contents(gold: i64, item_names: &[String]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if gold > 0 {
        parts.push(format!("{} gold", gold));
    }

    // Collapse duplicates in first-seen order: three identical arrows read as
    // "3 iron arrows", not as three separate list entries.
    let mut counts: Vec<(String, usize)> = Vec::new();
    for name in item_names {
        match counts.iter_mut().find(|(n, _)| n == name) {
            Some((_, c)) => *c += 1,
            None => counts.push((name.clone(), 1)),
        }
    }
    for (name, count) in counts {
        if count > 1 {
            parts.push(format!("{} {}", count, pluralize_item_name(&name)));
        } else {
            parts.push(name);
        }
    }

    match parts.len() {
        0 => None,
        1 => Some(parts.remove(0)),
        _ => {
            let last = parts.pop().expect("len >= 2");
            Some(format!("{} and {}", parts.join(", "), last))
        }
    }
}

/// Best-effort plural for a stacked item name. Item names carry an article
/// ("a rusty dagger"), which has to go before a count reads correctly.
fn pluralize_item_name(name: &str) -> String {
    let stripped = name
        .strip_prefix("a ")
        .or_else(|| name.strip_prefix("an "))
        .or_else(|| name.strip_prefix("the "))
        .unwrap_or(name);

    // Only the head noun pluralises: "pairs of boots", not "pair of bootss".
    let (head, tail) = match stripped.find(" of ") {
        Some(i) => (&stripped[..i], &stripped[i..]),
        None => (stripped, ""),
    };

    let plural_head = if head.ends_with('s')
        || head.ends_with("sh")
        || head.ends_with("ch")
        || head.ends_with('x')
        || head.ends_with('z')
    {
        format!("{}es", head)
    } else if head.ends_with('y') && !head.ends_with("ay") && !head.ends_with("ey") && !head.ends_with("oy") {
        format!("{}ies", &head[..head.len() - 1])
    } else {
        format!("{}s", head)
    };

    format!("{}{}", plural_head, tail)
}

/// Read a corpse and render its contents clause. `None` when the corpse is
/// gone or empty. Thin DB wrapper over [`format_corpse_contents`].
pub fn describe_corpse_contents(db: &crate::db::Db, corpse_id: &Uuid) -> Option<String> {
    let corpse = db.get_item_data(corpse_id).ok().flatten()?;
    let names: Vec<String> = corpse
        .container_contents
        .iter()
        .filter_map(|id| db.get_item_data(id).ok().flatten())
        .map(|item| item.name)
        .collect();
    format_corpse_contents(corpse.flags.corpse_gold, &names)
}

/// Milestone kill totals worth calling out. Dense early so a new player gets
/// several beats, then decade/century steps so it never becomes wallpaper.
pub fn is_kill_milestone(total: u32) -> bool {
    match total {
        0 => false,
        1 | 10 | 25 | 50 => true,
        n if n < 100 => false,
        n if n < 1000 => n % 100 == 0,
        n => n % 1000 == 0,
    }
}

/// English ordinal suffix for the milestone line ("1st", "2nd", "250th").
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{}{}", n, suffix)
}

// ---------------------------------------------------------------------------
// Decay warnings
// ---------------------------------------------------------------------------

/// Default decay-warning thresholds, as percentages of the decay window.
pub const DEFAULT_WARN_FRACTIONS: &str = "50,90";

/// Parse the `corpse_decay_warn_fractions` setting. Bad entries are dropped
/// rather than failing the whole setting — a typo should cost one warning, not
/// all of them. The result is sorted and deduplicated so the caller can rely on
/// ascending order.
pub fn parse_warn_fractions(raw: &str) -> Vec<i32> {
    let mut out: Vec<i32> = raw
        .split(',')
        .filter_map(|p| p.trim().parse::<i32>().ok())
        .filter(|p| (1..100).contains(p))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The threshold to warn at now, or `None` when there is nothing new to say.
///
/// Returns the *highest* crossed-but-unsent threshold, not the lowest: a server
/// that was down while a corpse aged past both 50% and 90% should say "almost
/// gone", not "half gone" followed by "almost gone" a minute later.
pub fn next_warning(age: i64, decay_time: i64, warned_pct: i32, fractions: &[i32]) -> Option<i32> {
    if decay_time <= 0 {
        return None;
    }
    let age_pct = ((age.max(0) as i128 * 100) / decay_time as i128) as i32;
    fractions
        .iter()
        .copied()
        .filter(|f| *f > warned_pct && age_pct >= *f)
        .max()
}

/// The line a player gets as their corpse ages. Blunt on purpose: the loss is
/// total and silence is what made it feel like an accident rather than a race.
pub fn decay_warning_line(remaining_secs: i64) -> String {
    let mins = (remaining_secs + 59) / 60;
    let when = if mins <= 1 {
        "less than a minute".to_string()
    } else {
        format!("about {} minutes", mins)
    };
    format!(
        "\x1b[1;31m[ Your corpse is rotting. {} left to reach it. ]\x1b[0m",
        when
    )
}

/// The line when a player's corpse is gone for good, and everything with it.
pub fn decay_final_line() -> &'static str {
    "\x1b[1;31m[ Your corpse has crumbled to dust. Whatever it held is gone. ]\x1b[0m"
}

// ---------------------------------------------------------------------------
// Loot protection
// ---------------------------------------------------------------------------

/// Seconds a player's corpse is reserved for them and their group.
pub const DEFAULT_LOOT_PROTECT_SECS: i64 = 300;

/// Whether a player corpse is still inside its protection window.
///
/// A non-positive `window` disables protection entirely, which is the escape
/// hatch for a server that wants the old rule back without a rebuild.
pub fn is_loot_protected(age: i64, window: i64) -> bool {
    window > 0 && age < window
}

/// The refusal a player gets for someone else's fresh corpse. Names the time
/// left rather than just saying no, so the rule is learnable from one attempt.
pub fn loot_protected_line(owner: &str, remaining_secs: i64) -> String {
    let mins = (remaining_secs.max(0) + 59) / 60;
    let when = if mins <= 1 {
        "under a minute".to_string()
    } else {
        format!("{} minutes", mins)
    };
    format!(
        "{}'s remains are still warm. You could not bring yourself to it for another {}.",
        owner, when
    )
}

#[cfg(test)]
mod loot_protection_tests {
    use super::*;

    #[test]
    fn protection_lapses_at_the_window() {
        assert!(is_loot_protected(0, 300));
        assert!(is_loot_protected(299, 300));
        assert!(!is_loot_protected(300, 300));
        assert!(!is_loot_protected(9999, 300));
    }

    /// The escape hatch: a server that wants full-loot-immediately back sets
    /// the window to zero rather than waiting for a code change.
    #[test]
    fn a_zero_window_protects_nothing() {
        assert!(!is_loot_protected(0, 0));
        assert!(!is_loot_protected(0, -1));
    }

    #[test]
    fn the_refusal_names_the_owner_and_the_wait() {
        let line = loot_protected_line("Kaleth", 120);
        assert!(line.contains("Kaleth"), "{line}");
        assert!(line.contains("2 minutes"), "{line}");
        assert!(loot_protected_line("Kaleth", 30).contains("under a minute"));
    }
}

#[cfg(test)]
mod decay_warning_tests {
    use super::*;

    #[test]
    fn fractions_parse_sorted_and_bounded() {
        assert_eq!(parse_warn_fractions("90,50"), vec![50, 90]);
        assert_eq!(parse_warn_fractions(" 50 , 50 , 75 "), vec![50, 75]);
        // 0 and 100 are meaningless: one fires instantly, the other coincides
        // with the corpse already being gone.
        assert_eq!(parse_warn_fractions("0,100,-5,50"), vec![50]);
        assert_eq!(parse_warn_fractions("nonsense"), Vec::<i32>::new());
        assert_eq!(parse_warn_fractions(DEFAULT_WARN_FRACTIONS), vec![50, 90]);
    }

    #[test]
    fn a_fresh_corpse_warns_about_nothing() {
        assert_eq!(next_warning(0, 3600, 0, &[50, 90]), None);
        assert_eq!(next_warning(1799, 3600, 0, &[50, 90]), None);
    }

    #[test]
    fn each_threshold_fires_once() {
        assert_eq!(next_warning(1800, 3600, 0, &[50, 90]), Some(50));
        // Already told them about 50%.
        assert_eq!(next_warning(1800, 3600, 50, &[50, 90]), None);
        assert_eq!(next_warning(3240, 3600, 50, &[50, 90]), Some(90));
        assert_eq!(next_warning(3599, 3600, 90, &[50, 90]), None);
    }

    /// A restart that spans several thresholds should produce the most urgent
    /// message, not a backlog of stale ones.
    #[test]
    fn a_long_gap_reports_the_most_urgent_threshold_only() {
        assert_eq!(next_warning(3400, 3600, 0, &[50, 90]), Some(90));
    }

    #[test]
    fn a_zero_window_is_inert_rather_than_dividing_by_it() {
        assert_eq!(next_warning(100, 0, 0, &[50, 90]), None);
    }

    #[test]
    fn the_warning_rounds_up_so_it_never_promises_more_time_than_there_is() {
        assert!(decay_warning_line(30).contains("less than a minute"));
        assert!(decay_warning_line(61).contains("about 2 minutes"));
        assert!(decay_warning_line(600).contains("about 10 minutes"));
        assert!(decay_warning_line(600).ends_with("\x1b[0m"));
    }
}

#[cfg(test)]
mod contents_tests {
    use super::*;

    #[test]
    fn empty_corpse_yields_no_line() {
        assert_eq!(format_corpse_contents(0, &[]), None);
    }

    #[test]
    fn gold_only() {
        assert_eq!(format_corpse_contents(37, &[]).as_deref(), Some("37 gold"));
    }

    #[test]
    fn gold_leads_and_the_last_item_gets_an_and() {
        let items = vec!["a chipped bone knife".to_string(), "an iron helm".to_string()];
        assert_eq!(
            format_corpse_contents(37, &items).as_deref(),
            Some("37 gold, a chipped bone knife and an iron helm")
        );
    }

    #[test]
    fn duplicates_collapse_into_a_count() {
        let items = vec!["a ration tin".to_string(), "a ration tin".to_string()];
        assert_eq!(format_corpse_contents(0, &items).as_deref(), Some("2 ration tins"));
    }

    #[test]
    fn zero_gold_is_not_listed() {
        let items = vec!["a rusty dagger".to_string()];
        assert_eq!(format_corpse_contents(0, &items).as_deref(), Some("a rusty dagger"));
    }

    #[test]
    fn plurals_handle_articles_sibilants_and_of_phrases() {
        assert_eq!(pluralize_item_name("a torch"), "torches");
        assert_eq!(pluralize_item_name("an iron flask"), "iron flasks");
        assert_eq!(pluralize_item_name("a pair of boots"), "pairs of boots");
        assert_eq!(pluralize_item_name("a ruby"), "rubies");
        assert_eq!(pluralize_item_name("a alloy"), "alloys");
        assert_eq!(pluralize_item_name("the glass"), "glasses");
    }

    #[test]
    fn milestones_are_dense_early_then_sparse() {
        let hits: Vec<u32> = (0..=2500).filter(|n| is_kill_milestone(*n)).collect();
        assert_eq!(
            hits,
            vec![1, 10, 25, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 2000]
        );
    }

    #[test]
    fn ordinals_handle_the_teens() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(13), "13th");
        assert_eq!(ordinal(250), "250th");
    }
}

#[cfg(test)]
mod gold_variance_tests {
    use super::mobile_gold_with_variance;

    #[test]
    fn small_value_varies_within_bounds() {
        // base=5 -> variance = round(1.0).max(1) = 1 -> [4, 6]
        for _ in 0..1000 {
            let rolled = mobile_gold_with_variance(5);
            assert!((4..=6).contains(&rolled), "rolled {rolled} out of [4,6]");
        }
    }

    #[test]
    fn zero_and_negative_yield_zero() {
        assert_eq!(mobile_gold_with_variance(0), 0);
        assert_eq!(mobile_gold_with_variance(-10), 0);
    }

    #[test]
    fn larger_value_uses_twenty_percent_spread() {
        // base=100 -> variance = 20 -> [80, 120]
        for _ in 0..1000 {
            let rolled = mobile_gold_with_variance(100);
            assert!((80..=120).contains(&rolled), "rolled {rolled} out of [80,120]");
        }
    }
}

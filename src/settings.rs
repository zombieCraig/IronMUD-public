//! Single source of truth for runtime-tunable setting defaults.
//!
//! Both the in-game `admin config` command (via the `get_setting_default`
//! Rhai binding) and the `ironmud-admin settings` CLI read defaults from this
//! table, so the displayed/effective defaults never drift between the two.
//!
//! These values mirror the fallbacks passed to `Db::get_setting_or_default`
//! (and equivalent inline `unwrap_or` defaults) at each consumption site.

/// All known settings with their default values, grouped by domain.
/// Displayed by `admin config` / `settings list` and used as the fallback
/// shown for unset keys.
pub const KNOWN_SETTINGS: &[(&str, &str)] = &[
    // Presets
    ("class_preset", "fantasy"),
    ("race_preset", "fantasy"),
    // Administration
    ("builder_mode", "granted"),
    ("motd", ""),
    ("recall_enabled", "true"),
    ("login_lockout_duration", "600"),
    ("idle_timeout_secs", "300"),
    ("starting_room_id", ""),
    // Regeneration
    ("stamina_regen_standing", "1"),
    ("stamina_regen_sitting", "3"),
    ("stamina_regen_sleeping", "5"),
    ("hp_regen_sitting", "1"),
    ("hp_regen_sleeping", "2"),
    ("mana_regen_standing", "1"),
    ("mana_regen_sitting", "2"),
    ("mana_regen_sleeping", "4"),
    // Stamina costs
    ("stamina_cost_move", "1"),
    ("stamina_cost_move_difficult", "2"),
    ("stamina_cost_attack", "5"),
    ("stamina_cost_recall", "50"),
    ("stamina_cost_flee", "10"),
    // Character needs
    ("thirst_base_rate", "1"),
    ("hunger_base_rate", "1"),
    // Corpse decay
    ("player_corpse_decay_secs", "3600"),
    ("mobile_corpse_decay_secs", "600"),
    // Economy & property
    ("rent_period_game_days", "30"),
    ("escrow_expiry_real_days", "30"),
    // Mail
    ("mail_stamp_price", "10"),
    ("mail_max_messages", "50"),
    ("mail_level_requirement", "5"),
    // Mob behavior
    ("wander_chance_percent", "33"),
    // Child safety
    ("min_attackable_age", "0"),
    ("conception_chance_per_day", "0.005"),
    ("adoption_chance_per_day", "0.10"),
    // Email
    ("email_verification_required", "false"),
    ("email_daily_cap", "20"),
    ("email_monthly_cap", "150"),
    ("email_verification_code_ttl_secs", "1800"),
];

/// Default value for a setting key, or `None` if the key is unknown.
pub fn setting_default(key: &str) -> Option<&'static str> {
    KNOWN_SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

//! Shared input-validation helpers for API request bodies.
//!
//! These helpers exist for *safety*, not game balance — they exist to keep
//! arithmetic from overflowing, persistence layers from being filled with
//! megabytes of free text, and obviously-broken values (zero / negative
//! shop rates, NaN, etc.) out of the world store. Game-design bounds
//! (e.g. "is +500 hit-bonus too strong for a level-3 area?") are an
//! admin-review-before-area-connection concern.
//!
//! All helpers return `ApiError::InvalidInput` so the API answers 400.

use super::error::ApiError;

/// Absolute ceiling for any item/mob stat bonus. Picked far below `i32::MAX`
/// so combat math (sums, multipliers) cannot wrap. Tune freely — anything
/// that keeps `bonus * roll + base` inside `i32` is fine.
pub const STAT_BONUS_ABS_MAX: i32 = 1_000_000;

/// Maximum dice count or sides on a damage-dice expression. Multiplied
/// every attack — must stay small enough that `count * sides` cannot
/// exhaust ticks or wrap.
pub const DICE_DIM_MAX: i32 = 1_000;

/// Shop rate floor / ceiling. Zero / negative would flip transaction signs
/// or hit divide-by-zero in inverse-rate paths. Ceiling keeps gold math
/// in-bounds for very expensive items.
pub const SHOP_RATE_MIN: i32 = 1;
pub const SHOP_RATE_MAX: i32 = 10_000;

/// Field-length ceilings for free-text inputs. Names and short descs are
/// shown in lists; long descs and bodies are paragraphs.
pub const NAME_MAX: usize = 80;
pub const SHORT_DESC_MAX: usize = 240;
pub const LONG_DESC_MAX: usize = 4 * 1024;
pub const DESCRIPTION_MAX: usize = 16 * 1024;
pub const TITLE_MAX: usize = 120;

/// Clamp-or-reject a stat bonus to the safety band. Rejects out-of-band
/// rather than silently clamping, so callers learn they're hitting it.
pub fn check_stat_bonus(field: &str, val: i32) -> Result<i32, ApiError> {
    if val.abs() > STAT_BONUS_ABS_MAX {
        return Err(ApiError::InvalidInput(format!(
            "{field} must be within ±{STAT_BONUS_ABS_MAX} (got {val})"
        )));
    }
    Ok(val)
}

/// Validate a damage-dice dimension (count or sides). Must be ≥ 1 and ≤ DICE_DIM_MAX.
pub fn check_dice_dim(field: &str, val: i32) -> Result<i32, ApiError> {
    if !(1..=DICE_DIM_MAX).contains(&val) {
        return Err(ApiError::InvalidInput(format!(
            "{field} must be between 1 and {DICE_DIM_MAX} (got {val})"
        )));
    }
    Ok(val)
}

/// Validate a shop rate (percent-style int, e.g. 100 = at-cost, 150 = +50%).
/// Rejects ≤ 0 (sign flips, divide-by-zero) and absurd ceilings.
pub fn check_shop_rate(field: &str, val: i32) -> Result<i32, ApiError> {
    if !(SHOP_RATE_MIN..=SHOP_RATE_MAX).contains(&val) {
        return Err(ApiError::InvalidInput(format!(
            "{field} must be between {SHOP_RATE_MIN} and {SHOP_RATE_MAX} (got {val})"
        )));
    }
    Ok(val)
}

/// A commission above 100% would pay the seller negative gold; a negative one
/// would mint it. Both are clamped at the write surface, in the same place and
/// for the same reason the achievement `trait_points` clamp lives here.
pub fn check_commission_pct(field: &str, val: i32) -> Result<i32, ApiError> {
    if !(0..=100).contains(&val) {
        return Err(ApiError::InvalidInput(format!(
            "{field} must be between 0 and 100 (got {val})"
        )));
    }
    Ok(val)
}

/// Reject text fields whose UTF-8 byte length exceeds `max`.
/// Longest vnum / identifier string.
pub const VNUM_MAX: usize = 64;
/// Longest DG script body (matches the in-game editor).
pub const DG_BODY_MAX: usize = 32 * 1024;
/// Most entries in a list field (keywords, seasons, links, stages, ...).
pub const LIST_MAX: usize = 64;
/// Most trusted builders on one area.
pub const TRUSTED_BUILDERS_MAX: usize = 50;
/// Largest request body the API accepts.
pub const REQUEST_BODY_MAX: usize = 1024 * 1024;

/// `check_text_len` for an optional field.
pub fn check_opt_len(field: &str, value: &Option<String>, max: usize) -> Result<(), ApiError> {
    match value {
        Some(v) => check_text_len(field, v, max),
        None => Ok(()),
    }
}

/// At most `max_items` entries, each at most `max_len` characters.
pub fn check_list(field: &str, values: &[String], max_items: usize, max_len: usize) -> Result<(), ApiError> {
    if values.len() > max_items {
        return Err(ApiError::InvalidInput(format!(
            "{field} has {} entries; the maximum is {max_items}",
            values.len()
        )));
    }
    for v in values {
        check_text_len(field, v, max_len)?;
    }
    Ok(())
}

/// At most `max` entries.
pub fn check_count<T>(field: &str, values: &[T], max: usize) -> Result<(), ApiError> {
    if values.len() > max {
        return Err(ApiError::InvalidInput(format!(
            "{field} has {} entries; the maximum is {max}",
            values.len()
        )));
    }
    Ok(())
}

pub fn check_text_len(field: &str, value: &str, max: usize) -> Result<(), ApiError> {
    if value.len() > max {
        return Err(ApiError::InvalidInput(format!(
            "{field} exceeds {max} bytes (got {})",
            value.len()
        )));
    }
    Ok(())
}

/// Reject a trigger `script_name` that could escape `scripts/triggers/`.
/// Allows `name`, `my-trigger_2`, and built-in `@template` names.
pub fn check_script_name(name: &str) -> Result<(), ApiError> {
    if crate::script::trigger_sandbox::is_valid_script_name(name) {
        Ok(())
    } else {
        Err(ApiError::InvalidInput(format!(
            "Invalid script_name '{name}': use letters, digits, '_' or '-' (optionally prefixed with '@' for a template)"
        )))
    }
}

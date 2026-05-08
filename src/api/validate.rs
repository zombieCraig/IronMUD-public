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

/// Reject text fields whose UTF-8 byte length exceeds `max`.
pub fn check_text_len(field: &str, value: &str, max: usize) -> Result<(), ApiError> {
    if value.len() > max {
        return Err(ApiError::InvalidInput(format!(
            "{field} exceeds {max} bytes (got {})",
            value.len()
        )));
    }
    Ok(())
}

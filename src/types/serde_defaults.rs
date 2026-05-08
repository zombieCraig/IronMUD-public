//! Shared serde default helpers used by multiple type modules.
//!
//! These helpers are imported into `types::mod` and become resolvable by name
//! inside `#[serde(default = "...")]` attributes on types defined there.
//! As types migrate into their own submodules, those modules can import these
//! helpers directly.

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_one() -> i32 {
    1
}

pub(crate) fn default_qty_one() -> i32 {
    1
}

pub(crate) fn default_stat() -> i32 {
    10 // Average stat value
}

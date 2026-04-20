//! Macros to reduce boilerplate in Rhai script registration

/// Register boolean flag getters and setters for a type.
///
/// This macro reduces the boilerplate for registering simple boolean flags
/// on Rhai types. Each flag gets a getter and setter registered.
///
/// # Example
///
/// ```ignore
/// register_bool_flags!(engine, ItemFlags,
///     no_drop, no_get, no_remove, invisible, glow
/// );
/// ```
///
/// Expands to:
/// ```ignore
/// engine
///     .register_get("no_drop", |f: &mut ItemFlags| f.no_drop)
///     .register_set("no_drop", |f: &mut ItemFlags, v: bool| f.no_drop = v)
///     .register_get("no_get", |f: &mut ItemFlags| f.no_get)
///     .register_set("no_get", |f: &mut ItemFlags, v: bool| f.no_get = v)
///     // ...etc
/// ```
#[macro_export]
macro_rules! register_bool_flags {
    ($engine:expr, $type:ty, $($flag:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($flag), |f: &mut $type| f.$flag)
            .register_set(stringify!($flag), |f: &mut $type, v: bool| f.$flag = v);
        )+
    }
}

/// Parse a UUID string, returning None on failure.
///
/// This is a helper function to reduce repetitive UUID parsing code
/// throughout the codebase.
///
/// # Example
///
/// ```ignore
/// if let Some(uuid) = parse_uuid_or_none(&id_str) {
///     // use uuid
/// }
/// ```
pub fn parse_uuid_or_none(id: &str) -> Option<uuid::Uuid> {
    uuid::Uuid::parse_str(id).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uuid_or_none() {
        // Valid UUID
        let valid = "00000000-0000-0000-0000-000000000001";
        assert!(parse_uuid_or_none(valid).is_some());

        // Invalid UUID
        let invalid = "not-a-uuid";
        assert!(parse_uuid_or_none(invalid).is_none());

        // Empty string
        assert!(parse_uuid_or_none("").is_none());
    }
}

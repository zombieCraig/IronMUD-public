//! Macros to reduce boilerplate in Rhai script registration

/// Register boolean flag getters and setters for a type.
///
/// Each flag gets a getter and setter registered on the engine.
///
/// # Example
///
/// ```ignore
/// register_bool_flags!(engine, ItemFlags,
///     no_drop, no_get, no_remove, invisible, glow
/// );
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

/// Register read-only boolean getters for fields on a type.
///
/// Use for derived/computed bools or fields the engine should not mutate.
#[macro_export]
macro_rules! register_bool_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field);
        )+
    }
}

/// Register String getter (clone) and setter (move) pairs for fields on a type.
#[macro_export]
macro_rules! register_string {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field.clone())
            .register_set(stringify!($field), |x: &mut $type, v: String| x.$field = v);
        )+
    }
}

/// Register read-only String getters (clone) for fields on a type.
#[macro_export]
macro_rules! register_string_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field.clone());
        )+
    }
}

/// Register `i32` field getter+setter pairs exposed as Rhai `i64`.
///
/// Rhai uses `i64` natively; this macro handles the cast in both directions.
#[macro_export]
macro_rules! register_i32 {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field as i64)
            .register_set(stringify!($field), |x: &mut $type, v: i64| x.$field = v as i32);
        )+
    }
}

/// Register read-only `i32` getter exposed as Rhai `i64`.
#[macro_export]
macro_rules! register_i32_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field as i64);
        )+
    }
}

/// Register `Option<String>` accessors: getter clones the inner string (empty if None),
/// setter wraps non-empty strings in `Some` and stores `None` for empty input.
#[macro_export]
macro_rules! register_option_string {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field.clone().unwrap_or_default())
            .register_set(stringify!($field), |x: &mut $type, v: String| {
                x.$field = if v.is_empty() { None } else { Some(v) };
            });
        )+
    }
}

/// Register read-only `Option<String>` getters (clone-or-empty).
#[macro_export]
macro_rules! register_option_string_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field.clone().unwrap_or_default());
        )+
    }
}

/// Register read-only `Uuid` getters that expose the value as a string.
#[macro_export]
macro_rules! register_uuid_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| x.$field.to_string());
        )+
    }
}

/// Register read-only `Option<Uuid>` getters that expose the value as a string
/// (empty string when `None`).
#[macro_export]
macro_rules! register_option_uuid_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| {
                x.$field.map(|u| u.to_string()).unwrap_or_default()
            });
        )+
    }
}

/// Register `Option<Uuid>` accessors: getter exposes a string (empty when None),
/// setter parses the string and clears the field on empty/invalid input.
#[macro_export]
macro_rules! register_option_uuid {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| {
                x.$field.map(|u| u.to_string()).unwrap_or_default()
            })
            .register_set(stringify!($field), |x: &mut $type, v: String| {
                x.$field = if v.is_empty() { None } else { uuid::Uuid::parse_str(&v).ok() };
            });
        )+
    }
}

/// Register a `Vec<String>` field as a Rhai `Array`. Getter clones strings out;
/// setter accepts an Array and filters non-string entries.
#[macro_export]
macro_rules! register_string_vec {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| {
                x.$field
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect::<rhai::Array>()
            })
            .register_set(stringify!($field), |x: &mut $type, v: rhai::Array| {
                x.$field = v
                    .into_iter()
                    .filter_map(|d| d.try_cast::<String>())
                    .collect();
            });
        )+
    }
}

/// Register a read-only `Vec<String>` field as a Rhai `Array`.
#[macro_export]
macro_rules! register_string_vec_ro {
    ($engine:expr, $type:ty, $($field:ident),+ $(,)?) => {
        $($engine
            .register_get(stringify!($field), |x: &mut $type| {
                x.$field
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect::<rhai::Array>()
            });
        )+
    }
}

/// Parse a UUID string, returning None on failure.
pub fn parse_uuid_or_none(id: &str) -> Option<uuid::Uuid> {
    uuid::Uuid::parse_str(id).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uuid_or_none() {
        let valid = "00000000-0000-0000-0000-000000000001";
        assert!(parse_uuid_or_none(valid).is_some());

        let invalid = "not-a-uuid";
        assert!(parse_uuid_or_none(invalid).is_none());

        assert!(parse_uuid_or_none("").is_none());
    }
}

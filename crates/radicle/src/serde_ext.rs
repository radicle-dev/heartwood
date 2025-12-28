pub mod bool {
    /// Function that always returns `true`, for use in `serde(default)` attributes.
    pub fn yes() -> bool {
        true
    }
}

pub mod string {
    use std::fmt::Display;
    use std::str::FromStr;

    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// Return true if the given value is the default for that type.
pub fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

/// Deserialize a value, but if it fails, return the default value.
pub fn ok_or_default<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: serde::Deserialize<'de> + Default,
    D: serde::Deserializer<'de>,
{
    let v: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

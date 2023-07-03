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

/// Unlike the default `serde` instances from `localtime`, this encodes and decodes using seconds
/// instead of milliseconds.
pub mod localtime {
    pub mod time {
        use localtime::LocalTime;
        use serde::{de, Deserialize, Deserializer, Serializer};

        pub fn serialize<S>(value: &LocalTime, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.collect_str(&value.as_secs())
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<LocalTime, D::Error>
        where
            D: Deserializer<'de>,
        {
            let seconds: u64 = String::deserialize(deserializer)?
                .parse()
                .map_err(de::Error::custom)?;

            Ok(LocalTime::from_secs(seconds))
        }
    }

    pub mod duration {
        use localtime::LocalDuration;
        use serde::{de, Deserialize, Deserializer, Serializer};

        pub fn serialize<S>(value: &LocalDuration, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.collect_str(&value.as_secs())
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<LocalDuration, D::Error>
        where
            D: Deserializer<'de>,
        {
            let seconds: u64 = String::deserialize(deserializer)?
                .parse()
                .map_err(de::Error::custom)?;

            Ok(LocalDuration::from_secs(seconds))
        }
    }
}

/// Return true if the given value is the default for that type.
pub fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

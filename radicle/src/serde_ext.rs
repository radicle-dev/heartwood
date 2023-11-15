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

/// Unlike the default `serde` instances from `localtime`, this encodes and decodes using seconds
/// instead of milliseconds.
pub mod localtime {
    pub mod time {
        use localtime::LocalTime;
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S>(value: &LocalTime, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_u64(value.as_secs())
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<LocalTime, D::Error>
        where
            D: Deserializer<'de>,
        {
            let seconds = u64::deserialize(deserializer)?;

            Ok(LocalTime::from_secs(seconds))
        }
    }

    pub mod option {
        pub mod time {
            use localtime::LocalTime;
            use serde::{Deserialize, Deserializer, Serializer};

            pub fn serialize<S>(value: &Option<LocalTime>, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                match value {
                    Some(time) => serializer.serialize_some(&time.as_secs()),
                    None => serializer.serialize_none(),
                }
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<LocalTime>, D::Error>
            where
                D: Deserializer<'de>,
            {
                let option = Option::<u64>::deserialize(deserializer)?;
                match option {
                    Some(seconds) => Ok(Some(LocalTime::from_secs(seconds))),
                    None => Ok(None),
                }
            }
        }
    }

    pub mod duration {
        use localtime::LocalDuration;
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S>(value: &LocalDuration, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_u64(value.as_secs())
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<LocalDuration, D::Error>
        where
            D: Deserializer<'de>,
        {
            let seconds = u64::deserialize(deserializer)?;

            Ok(LocalDuration::from_secs(seconds))
        }
    }
}

/// Return true if the given value is the default for that type.
pub fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

#[cfg(test)]
mod test {
    use super::*;

    use ::localtime::LocalTime;

    #[test]
    fn test_localtime() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
        struct Test {
            time: LocalTime,
        }
        let value = Test {
            time: LocalTime::from_millis(1699636852107),
        };

        assert_eq!(
            serde_json::from_str::<Test>(r#"{"time":1699636852107}"#).unwrap(),
            value
        );
        assert_eq!(
            serde_json::from_str::<Test>(serde_json::to_string(&value).unwrap().as_str()).unwrap(),
            value
        );
    }

    #[test]
    // Tests serialization into seconds instead of milliseconds.
    fn test_localtime_ext() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
        struct Test {
            #[serde(with = "localtime::time")]
            time: LocalTime,
        }
        let value = Test {
            time: LocalTime::from_secs(1699636852107),
        };

        assert_eq!(
            serde_json::from_str::<Test>(r#"{"time":1699636852107}"#).unwrap(),
            value
        );
        assert_eq!(
            serde_json::from_str::<Test>(serde_json::to_string(&value).unwrap().as_str()).unwrap(),
            value
        );
    }
}

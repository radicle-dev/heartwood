// Copyright © 2022 The Radicle Link Contributors

use std::str::FromStr;

use fmt::{Component, RefString};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The typename of an object. Valid typenames MUST be sequences of
/// alphanumeric characters or hyphens separated by a period. Each
/// component must start and end with an alphanumeric character.
///
/// The total length of a typename MUST NOT exceed 255, and each component
/// length MUST NOT exceed 63.
///
/// # Examples
///
/// * `abc.def`
/// * `xyz.rad.issues`
/// * `xyz.rad.patches.releases`
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TypeName(String);

impl TypeName {
    const MAX_LENGTH: usize = 255;
    const MAX_COMPONENT: usize = 63;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Error, Debug)]
#[error("the type name '{invalid}' is invalid")]
pub struct TypeNameParse {
    invalid: String,
}

impl FromStr for TypeName {
    type Err = TypeNameParse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > Self::MAX_LENGTH {
            return Err(TypeNameParse {
                invalid: s.to_string(),
            });
        }
        let split = s.split('.');
        for component in split {
            if component.len() > Self::MAX_COMPONENT {
                return Err(TypeNameParse {
                    invalid: s.to_string(),
                });
            }
            if component.is_empty() {
                return Err(TypeNameParse {
                    invalid: s.to_string(),
                });
            }
            if !component
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err(TypeNameParse {
                    invalid: s.to_string(),
                });
            }

            let first = component.chars().next().expect("component is not empty");
            let last = component.chars().last().expect("component is not empty");
            if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
                return Err(TypeNameParse {
                    invalid: s.to_string(),
                });
            }
        }
        Ok(TypeName(s.to_string()))
    }
}

impl From<&TypeName> for Component<'_> {
    fn from(name: &TypeName) -> Self {
        let refstr = RefString::try_from(name.0.to_string())
            .expect("collaborative object type names are valid ref strings");
        Component::from_refstr(refstr)
            .expect("collaborative object type names are valid refname components")
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr as _;

    use super::TypeName;

    #[test]
    fn valid_typenames() {
        assert!(TypeName::from_str("abc.def.ghi").is_ok());
        assert!(TypeName::from_str("abc.123.ghi").is_ok());
        assert!(TypeName::from_str("1bc.123.ghi").is_ok());
        assert!(TypeName::from_str("1bc-123.ghi").is_ok());
    }

    #[test]
    fn invalid_typenames() {
        assert!(TypeName::from_str("").is_err());
        assert!(TypeName::from_str(".").is_err());
        assert!(TypeName::from_str(".abc.123.ghi").is_err());
        assert!(TypeName::from_str("abc.123.ghi.").is_err());
        assert!(TypeName::from_str("abc..ghi").is_err());
        assert!(TypeName::from_str("abc.-123.ghi").is_err());
        assert!(TypeName::from_str("abc.123-.ghi").is_err());
        assert!(TypeName::from_str(&format!(
            "a.very.long.name.that.exceeds.the.two-hundred-and-fifty-five.length.limit.{}",
            "a".repeat(255)
        ))
        .is_err());
        assert!(TypeName::from_str(&format!(
            "component.exceeds.sixty-three.limit.{}",
            "a".repeat(64)
        ))
        .is_err());
    }
}

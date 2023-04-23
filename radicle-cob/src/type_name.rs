// Copyright © 2022 The Radicle Link Contributors

use std::{fmt, str::FromStr};

use git_ref_format::{Component, RefString};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The typename of an object. Valid typenames MUST be sequences of
/// alphanumeric characters separated by a period. The name must start
/// and end with an alphanumeric character
///
/// # Examples
///
/// * `abc.def`
/// * `xyz.rad.issues`
/// * `xyz.rad.patches.releases`
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TypeName(String);

impl TypeName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        let split = s.split('.');
        for component in split {
            if component.is_empty() {
                return Err(TypeNameParse {
                    invalid: s.to_string(),
                });
            }
            if !component.chars().all(char::is_alphanumeric) {
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
        assert!(TypeName::from_str(".abc.123.ghi").is_err());
        assert!(TypeName::from_str("abc.123.ghi.").is_err());
    }
}

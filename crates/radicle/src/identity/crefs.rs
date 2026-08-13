use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git::canonical::rules::{self, RawRules, Rules};
use crate::git::canonical::symbolic::{self, RawSymbolicRefs, SymbolicRefs};
use crate::git::fmt::Qualified;

use super::doc::{Delegates, Payload};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error(transparent)]
    Rules(#[from] rules::ValidationError),

    #[error(transparent)]
    Symbolic(#[from] symbolic::ValidationError),

    #[error("the target of the symbolic reference '{name} → {target}' is not matched by any rule")]
    Dangling {
        name: symbolic::RawName,
        target: Qualified<'static>,
    },

    #[error(
        "the symbolic reference name '{name}' is also matched by rule(s) with pattern(s) {patterns:?}"
    )]
    Clash {
        patterns: Vec<String>,
        name: Qualified<'static>,
    },
}

/// Configuration for canonical references and their rules.
///
/// [`RawCanonicalRefs`] are verified into [`CanonicalRefs`].
#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCanonicalRefs {
    rules: RawRules,

    #[serde(default)] // Default to empty for backwards compatibility.
    symbolic: RawSymbolicRefs,
}

impl RawCanonicalRefs {
    /// Construct a new [`RawCanonicalRefs`] from a set of [`RawRules`].
    pub fn new(rules: RawRules, symbolic: RawSymbolicRefs) -> Self {
        Self { rules, symbolic }
    }

    /// Return the [`RawRules`].
    pub fn raw_rules(&self) -> &RawRules {
        &self.rules
    }

    /// Return the [`RawRules`] for mutation.
    pub fn raw_rules_mut(&mut self) -> &mut RawRules {
        &mut self.rules
    }

    /// Return the [`RawSymbolicRefs`].
    pub fn symbolic(&self) -> &RawSymbolicRefs {
        &self.symbolic
    }

    /// Return the [`RawSymbolicRefs`] for mutation.
    pub fn symbolic_mut(&mut self) -> &mut RawSymbolicRefs {
        &mut self.symbolic
    }

    /// Validate the [`RawCanonicalRefs`] into a set of [`CanonicalRefs`].
    pub fn try_into_canonical_refs<R>(
        self,
        resolve: &mut R,
    ) -> Result<CanonicalRefs, ValidationError>
    where
        R: Fn() -> Delegates,
    {
        let rules = Rules::from_raw(self.rules, resolve)?;
        let symbolic = SymbolicRefs::try_from(self.symbolic)?;
        CanonicalRefs::new(rules, symbolic)
    }
}

/// Configuration for canonical references and their [`Rules`].
///
/// [`CanonicalRefs`] can be converted into a [`Payload`] using its [`From`]
/// implementation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalRefs {
    rules: Rules,

    #[serde(default, skip_serializing_if = "SymbolicRefs::is_empty")]
    symbolic: SymbolicRefs,
}

impl CanonicalRefs {
    /// Construct a new [`CanonicalRefs`] from a set of [`Rules`] and
    /// [`SymbolicRefs`], validating that these may be evaluated to a well
    /// formed set of references when interpreted together.
    pub fn new(rules: Rules, symbolic: SymbolicRefs) -> Result<Self, ValidationError> {
        for (name, target) in symbolic.iter_resolved() {
            if rules.matches(target).next().is_none() {
                return Err(ValidationError::Dangling {
                    name: name.to_owned(),
                    target: target.to_owned(),
                });
            }

            let Some(name) = Qualified::from_refstr(name) else {
                continue;
            };

            let mut patterns = rules
                .matches(&name)
                .map(|(pattern, _)| pattern.to_string())
                .peekable();
            if patterns.peek().is_some() {
                return Err(ValidationError::Clash {
                    patterns: patterns.collect(),
                    name: name.to_owned(),
                });
            }
        }

        Ok(CanonicalRefs { rules, symbolic })
    }

    /// Return the [`Rules`].
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// Return the [`SymbolicRefs`].
    pub fn symbolic(&self) -> &SymbolicRefs {
        &self.symbolic
    }
}

impl Extend<(rules::RawPattern, rules::RawRule)> for RawCanonicalRefs {
    fn extend<T: IntoIterator<Item = (rules::RawPattern, rules::RawRule)>>(&mut self, iter: T) {
        self.rules.extend(iter)
    }
}

impl From<CanonicalRefs> for RawCanonicalRefs {
    fn from(crefs: CanonicalRefs) -> Self {
        Self {
            rules: crefs.rules.into(),
            symbolic: crefs.symbolic.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CanonicalRefsPayloadError {
    #[error("could not convert canonical references to JSON: {0}")]
    Json(#[source] serde_json::Error),
}

impl TryFrom<CanonicalRefs> for Payload {
    type Error = CanonicalRefsPayloadError;

    fn try_from(crefs: CanonicalRefs) -> Result<Self, Self::Error> {
        let value = serde_json::to_value(crefs).map_err(CanonicalRefsPayloadError::Json)?;
        Ok(Self::from(value))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use crate::assert_matches;

    use super::{ValidationError::*, *};

    fn from(value: serde_json::Value) -> Result<CanonicalRefs, super::ValidationError> {
        let delegates: Delegates = crate::test::arbitrary::r#gen::<crate::prelude::Did>(1).into();
        serde_json::from_value::<RawCanonicalRefs>(value)
            .unwrap()
            .try_into_canonical_refs(&mut || delegates.clone())
    }

    /// Backwards compatibility to before addition of symbolic references.
    #[test]
    fn omit_symbolic() {
        assert_matches!(
            from(json!({
                "rules": {},
            })),
            Ok(_)
        );
    }

    #[test]
    fn invalid_dangling() {
        assert_matches!(
            from(json!({
                "symbolic": {
                    "HEAD": "refs/heads/master"
                },
                "rules": {},
            })),
            Err(Dangling { .. })
        );
    }

    #[test]
    fn invalid_clash() {
        assert_matches!(
            from(json!({
                "symbolic": {
                    "refs/heads/foo": "refs/heads/bar",
                },
                "rules": {
                    "refs/heads/foo": {
                        "allow": "delegates",
                        "threshold": 1,
                    },
                    "refs/heads/bar": {
                        "allow": "delegates",
                        "threshold": 1,
                    },
                },
            })),
            Err(Clash { .. })
        );
    }

    #[test]
    fn invalid_clash_asterisk_name() {
        assert_matches!(
            from(json!({
                "symbolic": {
                    "refs/heads/foo": "refs/heads/bar",
                },
                "rules": {
                    "refs/heads/*": {
                        "allow": "delegates",
                        "threshold": 1,
                    },
                },
            })),
            Err(Clash { .. })
        );
    }

    #[test]
    fn valid_asterisk_target() {
        assert_matches!(
            from(json!({
                "symbolic": {
                    "HEAD": "refs/heads/master",
                },
                "rules": {
                    "refs/heads/*": {
                        "allow": "delegates",
                        "threshold": 1,
                    },
                },
            })),
            Ok(_)
        );
    }

    #[test]
    fn valid() {
        assert_matches!(
            from(json!({
                "symbolic": {
                    "refs/heads/foo": "refs/heads/ruled/bar",
                },
                "rules": {
                    "refs/heads/ruled/*": {
                        "allow": "delegates",
                        "threshold": 1,
                    },
                },
            })),
            Ok(_)
        );
    }
}

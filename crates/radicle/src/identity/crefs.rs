use serde::{Deserialize, Serialize};

use crate::git::canonical::{
    rules::{self, RawRules, Rules, ValidationError},
    ValidRule,
};

use super::doc::{Delegates, Payload};

/// Implemented by any data type or store that can return [`CanonicalRefs`] and
/// [`RawCanonicalRefs`].
pub trait GetCanonicalRefs {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieve the [`CanonicalRefs`], returning `Some` if they are not
    /// present, and `None` if they are missing.
    ///
    /// [`Self::Error`] is used to return any domain-specific error by the
    /// implementing type.
    fn canonical_refs(&self) -> Result<Option<CanonicalRefs>, Self::Error>;

    /// Retrieve the [`RawCanonicalRefs`], returning `Some` if they are not
    /// present, and `None` if they are missing.
    ///
    /// [`Self::Error`] is used to return any domain-specific error by the
    /// implementing type.
    fn raw_canonical_refs(&self) -> Result<Option<RawCanonicalRefs>, Self::Error>;

    /// Retrieve the [`CanonicalRefs`], and in the case of `None`, then use the
    /// `default` function to return a default set of [`CanonicalRefs`].
    fn canonical_refs_or_default<D, E>(&self, default: D) -> Result<CanonicalRefs, E>
    where
        D: Fn() -> Result<CanonicalRefs, E>,
        E: From<Self::Error>,
    {
        match self.canonical_refs()? {
            Some(crefs) => Ok(crefs),
            None => Ok(default()?),
        }
    }
}

/// Configuration for canonical references and their rules.
///
/// `RawCanonicalRefs` are verified into [`CanonicalRefs`].
#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCanonicalRefs {
    rules: RawRules,
}

impl RawCanonicalRefs {
    /// Construct a new [`RawCanonicalRefs`] from a set of [`RawRules`].
    pub fn new(rules: RawRules) -> Self {
        Self { rules }
    }

    /// Return the [`RawRules`].
    pub fn raw_rules(&self) -> &RawRules {
        &self.rules
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
        Ok(CanonicalRefs::new(rules))
    }
}

/// Configuration for canonical references and their [`Rules`].
///
/// [`CanonicalRefs`] can be converted into a [`Payload`] using its [`From`]
/// implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalRefs {
    rules: Rules,
}

impl CanonicalRefs {
    /// Construct a new [`CanonicalRefs`] from a set of [`Rules`].
    pub fn new(rules: Rules) -> Self {
        CanonicalRefs { rules }
    }

    /// Return the [`Rules`].
    pub fn rules(&self) -> &Rules {
        &self.rules
    }
}

impl FromIterator<(rules::Pattern, ValidRule)> for CanonicalRefs {
    fn from_iter<T: IntoIterator<Item = (rules::Pattern, ValidRule)>>(iter: T) -> Self {
        Self::new(Rules::from_iter(iter))
    }
}

impl Extend<(rules::Pattern, ValidRule)> for CanonicalRefs {
    fn extend<T: IntoIterator<Item = (rules::Pattern, ValidRule)>>(&mut self, iter: T) {
        self.rules.extend(iter)
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

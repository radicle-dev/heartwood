use serde::{Deserialize, Serialize};

use crate::git::canonical::rules::{RawRules, Rules, ValidationError};

use super::doc::{Delegates, Payload};

/// Implemented by any data type or store that can return [`CanonicalRefs`] and
/// [`RawCanonicalRefs`].
pub trait GetRawCanonicalRefs {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieve the [`RawCanonicalRefs`], returning `Some` if they are
    /// present, and `None` if they are absent.
    ///
    /// [`Self::Error`] is used to return any domain-specific error by the
    /// implementing type.
    fn raw_canonical_refs(&self) -> Result<Option<RawCanonicalRefs>, Self::Error>;
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

// Weird lint, see <https://github.com/rust-lang/rust-clippy/issues/14275>
#![allow(clippy::doc_overindented_list_items)]

//! Implementation of RIP-0004 Canonical References
//!
//! [`RawRules`] is intended to be deserialized and then validated into a set of
//! [`Rules`]. These can then be used to see if a [`Qualified`] reference
//! matches any of the rules, using [`Rules::matches`]. Using [`Canonical`] with
//! the first matched rule, and this can be used to calculate the
//! [`Canonical::quorum`].

#[cfg(test)]
mod test;

use std::cmp::Ordering;
use std::collections::BTreeMap;

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use serde_json as json;
use thiserror::Error;

use crate::git;
use crate::git::canonical;
use crate::git::canonical::Canonical;
use crate::git::fmt::Qualified;
use crate::git::fmt::refspec::QualifiedPattern;
use crate::identity::{Did, doc};

use super::protect;
use super::protect::Unprotected;

const ASTERISK: char = '*';

/// Private trait to ensure that not any [`Rule`] can be deserialized.
/// Implementations are provided for [`Allowed`] and [`usize`] so that [`RawRule`]s
/// can be deserialized, while [`ValidRule`]s cannot – preventing deserialization
/// bugs for that type.
trait Sealed {}
impl Sealed for Allowed {}
impl Sealed for usize {}

/// A pattern for a rule is an unprotected qualified reference pattern.
/// Requiring [`Unprotected`] makes rules that would create protected references
/// unrepresentable.
pub(super) type Pattern = Unprotected<QualifiedPattern<'static>>;

pub type RawPattern = QualifiedPattern<'static>;

/// Check if the `pattern` matches the `refname`.
fn matches(pattern: &RawPattern, refname: &Qualified) -> bool {
    // N.b. Git's refspecs do not quite match with glob-star semantics. A
    // single `*` in a refspec is expected to match all references under
    // that namespace, even if they are further down the hierarchy.
    // Thus, the following rules are applied:
    //
    //   - a trailing `*` matches anything that starts with the prefix
    //   - a `*` in between path components changes to `**`
    match pattern.as_str().split_once(ASTERISK) {
        None => pattern.as_str() == refname.as_str(),
        Some((prefix, "")) => refname.as_str().starts_with(prefix),
        Some((prefix, suffix)) => {
            let mut spec = prefix.to_string();
            spec.push_str("**");
            spec.push_str(suffix);
            fast_glob::glob_match(&spec, refname.as_str())
        }
    }
}

/// Patterns are ordered by their specificity.
///
/// This is heavily influenced by the evaluation priority of Rules. For a
/// candidate reference name, we want the rule associated with the most specific
/// pattern to apply, i.e. to take priority over all other rules with less
/// specific patterns.
///
/// For two patterns `φ` and `ψ`, we say that "`φ` is more specific than `ψ`", denoted
/// `φ < ψ` if:
///
///  1. The number of components in `φ` is larger than the number of components
///     in `ψ`. (Note that the number of components is equal to the number of
///     occurrences of the symbol '/' in the pattern, plus 1).
///     The justification is, that refnames might be interpreted as a hierarchy
///     where a match on more components would mean a match at a lower level in
///     the hierarchy, thus being more specific.
///     Imagine a refname hierarchy that maps to a corporate hierarchy.
///     The pattern "department-1" matches all refnames that are administered
///     by a particular department, and thus is not very specific.
///     To contrast, the pattern "department-1/team-a/project-i/nice-feature"
///     is very specific as it matches all refnames that relate to the
///     development of a particular feature for a particular project by a
///     particular team.
///     Note that this would also apply when the connection between the `φ` and `ψ`
///     is not as obvious, e.g. also `a/b/c/d/* < */x`.
///
/// (Note that for the following items, one may assume that `φ` and `ψ` have the
/// same number of components.)
///
///  2. If path component i of `φ`, denoted `φ[i]`, is more specific than path
///     component i of `ψ`, denoted `ψ[i]`. This is the case if:
///      a. `φ[i]` does not contain an asterisk and `ψ[i]` contains an asterisk,
///         i.e. the symbol `*`, e.g. `a < * and abc < a*`.
///         Note that this is important to capture specificity across
///         components, i.e. to conclude that `a/b/* < a/*/c`.
///      b. Both `φ[i]` and `ψ[i]` contain an asterisk.
///          A. The asterisk in `φ[i]` is further right than the asterisk in `φ[i]`,
///             e.g. `aa* < a*`.
///          B. The asterisk in `φ[i]` and `ψ[i]` is equally far to the right,
///             and `φ[i]` is longer than `ψ[i]`, e.g. `a*b < a*`.
///
///  3. Otherwise, fall back to a lexicographic ordering.
///
/// Some examples (justification in parentheses):
///
/// ```text, no_run
/// refs/tags/release/candidates/* <(1.)   refs/tags/release/* <(1.) refs/tags/*
/// refs/tags/v1.0                 <(2.a.) refs/tags/*
/// refs/heads/*                   <(3.)   refs/tags/*
/// refs/heads/main                <(3.)   refs/tags/v1.0
/// ```
impl Ord for Pattern {
    fn cmp(&self, other: &Self) -> Ordering {
        #[derive(Debug, Clone, Copy)]
        #[repr(i8)]
        enum ComponentOrdering {
            MatchLength(Ordering),
            Lexicographic(Ordering),
        }

        impl ComponentOrdering {
            fn merge(&mut self, other: Self) {
                *self = match (*self, other) {
                    (Self::Lexicographic(Ordering::Equal), Self::Lexicographic(other)) => {
                        Self::Lexicographic(other)
                    }
                    (Self::Lexicographic(_), Self::MatchLength(other)) => Self::MatchLength(other),
                    (Self::MatchLength(Ordering::Equal), Self::MatchLength(other)) => {
                        Self::MatchLength(other)
                    }
                    (clone, _) => clone,
                }
            }
        }

        impl From<ComponentOrdering> for Ordering {
            fn from(value: ComponentOrdering) -> Self {
                match value {
                    ComponentOrdering::MatchLength(ordering) => ordering,
                    ComponentOrdering::Lexicographic(ordering) => ordering,
                }
            }
        }

        impl Default for ComponentOrdering {
            /// The weakest value of Self, which will be absorbed by any
            /// other in [`ComponentOrdering::merge`].
            fn default() -> Self {
                Self::Lexicographic(Ordering::Equal)
            }
        }

        use git::fmt::refspec::Component;

        fn cmp_component(lhs: Component<'_>, rhs: Component<'_>) -> ComponentOrdering {
            let (l, r) = (lhs.as_str(), rhs.as_str());
            match (l.find(ASTERISK), r.find(ASTERISK)) {
                // (2.a.)
                (Some(_), None) => ComponentOrdering::MatchLength(Ordering::Greater),
                // (2.a.)
                (None, Some(_)) => ComponentOrdering::MatchLength(Ordering::Less),
                (Some(li), Some(ri)) => {
                    if li != ri {
                        // (2.b.A)
                        ComponentOrdering::MatchLength(li.cmp(&ri).reverse())
                    } else if l.len() != r.len() {
                        // (2.b.B)
                        ComponentOrdering::MatchLength(l.len().cmp(&r.len()).reverse())
                    } else {
                        // (3.)
                        ComponentOrdering::Lexicographic(l.cmp(r))
                    }
                }
                // (3.)
                (None, None) => ComponentOrdering::Lexicographic(l.cmp(r)),
            }
        }

        let mut result = ComponentOrdering::default();
        let mut lhs = self.as_ref().components();
        let mut rhs = other.as_ref().components();
        loop {
            match (lhs.next(), rhs.next()) {
                (None, Some(_)) => return Ordering::Greater, // (1.)
                (Some(_), None) => return Ordering::Less,    // (1.)
                (Some(lhs), Some(rhs)) => {
                    result.merge(cmp_component(lhs, rhs));
                }
                (None, None) => return result.into(),
            }
        }
    }
}

impl PartialOrd for Pattern {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A [`Rule`] that can be serialized and deserialized safely.
///
/// Should be converted to a [`ValidRule`] via [`Rule::validate`].
pub type RawRule = Rule<Allowed, usize>;

impl RawRule {
    /// Validate the `Rule` into a form that can be used for calculating
    /// canonical references.
    ///
    /// The `resolve` callback is to allow the caller to specify the DIDs of the
    /// identity document, in the case that the allowed value is
    /// [`Allowed::Delegates`].
    pub fn validate<R>(self, resolve: &mut R) -> Result<ValidRule, ValidationError>
    where
        R: Fn() -> doc::Delegates,
    {
        let Self {
            allow: delegates,
            threshold,
            ..
        } = self;
        let allow = match &delegates {
            Allowed::Delegates => ResolvedDelegates::Delegates(resolve()),
            Allowed::Set(delegates) => {
                let valid =
                    doc::Delegates::new(delegates.clone()).map_err(ValidationError::from)?;
                ResolvedDelegates::Set(valid)
            }
        };
        let threshold = doc::Threshold::new(threshold, &allow)?;
        Ok(Rule {
            allow,
            threshold,
            extensions: self.extensions,
        })
    }
}

/// A set of [`RawRule`]s that can be serialized and deserialized.
///
/// Note that matching on the [`RawPattern`]s is deliberately not implemented,
/// as the [`RawRule`]s are not validated and thus should not be used to
/// calculate canonical references.
/// If you want to match on a set of rules, obtain [`Rules`] via
/// [`Rules::from_raw`], and use [`Rules::matches`] on the result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawRules {
    /// The reference pattern that this rule applies to.
    ///
    /// Note that this can be a fully-qualified pattern, e.g. `refs/heads/qa`,
    /// as well as a wild-card pattern, e.g. `refs/tags/*`.
    #[serde(flatten)]
    pub rules: BTreeMap<RawPattern, RawRule>,
}

impl RawRules {
    /// Returns an iterator over the [`RawPattern`] and [`RawRule`]
    /// in the set of rules.
    pub fn iter(&self) -> impl Iterator<Item = (&RawPattern, &RawRule)> {
        self.rules.iter()
    }

    /// Add a new [`RawRule`] to the set of rules.
    ///
    /// Returns the replaced rule, if it existed.
    pub fn insert(&mut self, pattern: RawPattern, rule: RawRule) -> Option<RawRule> {
        self.rules.insert(pattern, rule)
    }

    /// Remove the rule that matches the `pattern` parameter.
    ///
    /// Returns the rule if it existed.
    pub fn remove(&mut self, pattern: &RawPattern) -> Option<RawRule> {
        self.rules.remove(pattern)
    }

    /// Check to see if there is an exact match for `refname` in the rules.
    pub fn exact_match(&self, refname: &Qualified) -> bool {
        let refname = refname.as_str();
        self.rules
            .iter()
            .any(|(pattern, _)| pattern.as_str() == refname)
    }
}

impl Extend<(RawPattern, RawRule)> for RawRules {
    fn extend<T: IntoIterator<Item = (RawPattern, RawRule)>>(&mut self, iter: T) {
        self.rules.extend(iter)
    }
}

impl From<BTreeMap<RawPattern, RawRule>> for RawRules {
    fn from(rules: BTreeMap<RawPattern, RawRule>) -> Self {
        RawRules { rules }
    }
}

impl FromIterator<(RawPattern, RawRule)> for RawRules {
    fn from_iter<T: IntoIterator<Item = (RawPattern, RawRule)>>(iter: T) -> Self {
        iter.into_iter().collect::<BTreeMap<_, _>>().into()
    }
}

impl IntoIterator for RawRules {
    type Item = (RawPattern, RawRule);
    type IntoIter = std::collections::btree_map::IntoIter<RawPattern, RawRule>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules.into_iter()
    }
}

/// A [`Rule`] that has been validated. See [`Rules`] and [`Rules::matches`] for
/// its main usage.
///
/// N.b. a `ValidRule` can be serialized, however, it cannot be deserialized.
/// This is due to the fact that the `allow` field may have a value of
/// `delegates`. In those cases the value needs to be looked up via the identity
/// document and validated.
pub type ValidRule = Rule<ResolvedDelegates, doc::Threshold>;

impl From<ValidRule> for RawRule {
    fn from(rule: ValidRule) -> Self {
        let Rule {
            allow,
            threshold,
            extensions,
        } = rule;
        Self {
            allow: allow.into(),
            threshold: threshold.into(),
            extensions,
        }
    }
}

/// A representation of a set of allowed DIDs.
///
/// `Allowed` is used in a `RawRule`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Allowed {
    /// Pointer to the identity document's set of delegates.
    #[serde(rename = "delegates")]
    #[default]
    Delegates,
    /// Explicit list of allowed DIDs.
    ///
    /// The elements of the list of allowed DIDs will be made unique, i.e.
    /// duplicate DIDs will be discarded.
    ///
    /// # Validation
    ///
    /// The list of allowed DIDs, `allowed`, must satisfy:
    /// ```text
    /// 1 <= allowed.len() <= 255
    /// ```
    #[serde(untagged)]
    Set(NonEmpty<Did>),
}

impl From<NonEmpty<Did>> for Allowed {
    fn from(dids: NonEmpty<Did>) -> Self {
        Self::Set(dids)
    }
}

impl From<Did> for Allowed {
    fn from(did: Did) -> Self {
        Self::Set(NonEmpty::new(did))
    }
}

impl std::fmt::Display for Allowed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Allowed::Delegates => f.write_str("\"delegates\""),
            Allowed::Set(dids) => {
                let dids = dids
                    .iter()
                    .map(|did| did.to_string())
                    .collect::<Vec<_>>()
                    .join("\", \"");
                f.write_fmt(format_args!("[\"{dids}\"]"))
            }
        }
    }
}

/// A marker `enum` that is used in a [`ValidRule`].
///
/// It ensures that a rule that has been deserialized, resolving the `delegates`
/// token to a set of DIDs, is still serialized back to the `delegates` token –
/// as opposed to serializing it to the set of DIDs.
///
/// The variants mirror the [`Allowed::Delegates`] and [`Allowed::Set`]
/// variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(into = "Allowed")]
pub enum ResolvedDelegates {
    Delegates(doc::Delegates),
    Set(doc::Delegates),
}

impl From<ResolvedDelegates> for Allowed {
    fn from(ds: ResolvedDelegates) -> Self {
        match ds {
            ResolvedDelegates::Delegates(_) => Self::Delegates,
            ResolvedDelegates::Set(ds) => Self::Set(ds.into()),
        }
    }
}

impl std::ops::Deref for ResolvedDelegates {
    type Target = doc::Delegates;

    fn deref(&self) -> &Self::Target {
        match self {
            ResolvedDelegates::Delegates(ds) => ds,
            ResolvedDelegates::Set(ds) => ds,
        }
    }
}

/// A reference that has been matched against a [`ValidRule`].
///
/// Can be constructed by using [`Rules::matches`].
#[derive(Debug)]
pub struct MatchedRule<'a> {
    refname: Qualified<'a>,
    rule: ValidRule,
}

impl MatchedRule<'_> {
    /// Return the reference name that was used for checking if it was a match.
    pub fn refname(&self) -> &Qualified<'_> {
        &self.refname
    }

    /// Return the rule that was matched.
    pub fn rule(&self) -> &ValidRule {
        &self.rule
    }

    /// Return the allowed DIDs for the matched rule.
    pub fn allowed(&self) -> &doc::Delegates {
        self.rule().allowed()
    }

    /// Return the [`doc::Threshold`] for the matched rule.
    pub fn threshold(&self) -> &doc::Threshold {
        self.rule().threshold()
    }
}

/// A set of valid [`Rule`]s, where the set of DIDs and threshold are fully
/// resolved and valid. Since the rules are constructed via a `BTreeMap`, they
/// cannot be duplicated.
///
/// To construct the set of rules, use [`Rules::from_raw`], which validates a
/// set of [`RawRule`]s, and their [`RawPattern`] references, into a set of
/// [`ValidRule`]s.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Rules {
    #[serde(flatten)]
    rules: BTreeMap<Pattern, ValidRule>,
}

impl FromIterator<(Pattern, ValidRule)> for Rules {
    fn from_iter<T: IntoIterator<Item = (Pattern, ValidRule)>>(iter: T) -> Self {
        Self {
            rules: iter.into_iter().collect(),
        }
    }
}

impl From<Rules> for RawRules {
    fn from(Rules { rules }: Rules) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|(pattern, rule)| (pattern.into_inner(), rule.into()))
                .collect(),
        }
    }
}

impl Rules {
    /// Returns `true` is the set of rules is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Construct a set of `Rules` given a set of `RawRule`s.
    pub fn from_raw<R>(
        rules: impl IntoIterator<Item = (RawPattern, RawRule)>,
        resolve: &mut R,
    ) -> Result<Self, ValidationError>
    where
        R: Fn() -> doc::Delegates,
    {
        let valid = rules
            .into_iter()
            .map(|(pattern, rule)| {
                let pattern = Unprotected::new(pattern)?;
                rule.validate(resolve).map(|rule| (pattern, rule))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { rules: valid })
    }

    /// Return the matching rules for the given `refname`.
    pub fn matches<'a>(
        &self,
        refname: &Qualified<'a>,
    ) -> impl Iterator<Item = (&RawPattern, &ValidRule)> + use<'a, '_> {
        let refname_cloned = refname.clone();
        self.rules
            .iter()
            .filter(move |(pattern, _)| matches(pattern.as_ref(), &refname_cloned))
            .map(|(pattern, rule)| (pattern.as_ref(), rule))
    }

    /// Match given refname, take the most specific rule, and prepare evaluation
    /// as [`Canonical`]
    ///
    /// N.b. it will find the first rule that is most specific for the given
    /// `refname`.
    pub fn canonical<'a, 'b, 'r, R>(
        &'a self,
        refname: Qualified<'b>,
        repo: &'r R,
    ) -> Option<Canonical<'b, 'a, 'r, R, canonical::Initial>>
    where
        R: canonical::effects::Ancestry
            + canonical::effects::FindMergeBase
            + canonical::effects::FindObjects,
    {
        self.matches(&refname)
            .next()
            .map(|(_, rule)| Canonical::new(refname, rule, repo))
    }
}

/// A `Rule` defines how a reference or set of references can be made canonical,
/// i.e. have a top-level `refs/*` entry – see [`RawPattern`].
///
/// The [`Rule::allowed`] type is generic to allow for [`Allowed`] to be used
/// for serialization and deserialization, however, the use of
/// [`Rule::validate`] should be used to get a valid rule.
///
/// The [`Rule::threshold`], similarly, allows for [`doc::Threshold`] to be used, and
/// [`Rule::validate`] should be used to get a valid rule.
// N.b. it's safe to derive `Serialize` since we only allow constructing a
// `Rule` via `Rule::validate`, and we seal `Deserialize` by ensuring that only
// `RawRule` can be deserialized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "D: Sealed + Deserialize<'de>, T: Sealed + Deserialize<'de>"))]
pub struct Rule<D, T> {
    /// The set of allowed DIDs that are considered for voting for this rule.
    allow: D,
    /// The threshold the votes must pass for the reference(s) to be considered
    /// canonical.
    threshold: T,

    /// Optional extensions in rules. This is intended to preserve backwards and
    /// forward-compatibility
    #[serde(skip_serializing_if = "json::Map::is_empty")]
    #[serde(flatten)]
    extensions: json::Map<String, json::Value>,
}

impl<D, T> Rule<D, T> {
    /// Construct a new `Rule` with the given `allow` and `threshold`.
    pub fn new(allow: D, threshold: T) -> Self {
        Self {
            allow,
            threshold,
            extensions: json::Map::new(),
        }
    }

    /// Get the set of DIDs this `Rule` was created with.
    pub fn allowed(&self) -> &D {
        &self.allow
    }

    /// Get the set of threshold this `Rule` was created with.
    pub fn threshold(&self) -> &T {
        &self.threshold
    }

    /// Get the extensions that may have been added to this `Rule`.
    pub fn extensions(&self) -> &json::Map<String, json::Value> {
        &self.extensions
    }

    /// If the [`Rule::extensions`] is not set, the provided `extensions` will
    /// be used.
    ///
    /// Otherwise, it expects that the JSON value is a `Map` and the
    /// `extensions` are merged. If the existing value is any other kind of JSON
    /// value, this is a no-op.
    pub fn add_extensions(&mut self, extensions: impl Into<json::Map<String, json::Value>>) {
        self.extensions.extend(extensions.into());
    }
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error(transparent)]
    Threshold(#[from] doc::ThresholdError),
    #[error(transparent)]
    Delegates(#[from] doc::DelegatesError),
    #[error(transparent)]
    Protected(#[from] protect::Error),
}

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error(transparent)]
    Git(#[from] crate::git::raw::Error),
}

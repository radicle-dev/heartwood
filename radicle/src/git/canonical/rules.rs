//! Implementation of RIP-0004 Canonical References
//!
//! [`RawRules`] is intended to be deserialized and then validated into a set of
//! [`Rules`]. These can then be used to see if a [`Qualified`] reference
//! matches any of the rules. If so, a [`MatchedRule`] is returned, which can be
//! used to construct a [`super::Canonical`], and finally can be used to calculate the
//! [`super::Canonical::quorum`].

use core::fmt;
use std::cmp::Ordering;
use std::collections::BTreeMap;

use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json as json;
use thiserror::Error;

use crate::git;
use crate::git::canonical::Canonical;
use crate::git::fmt::{refname, RefString};
use crate::git::refspec::QualifiedPattern;
use crate::git::Qualified;
use crate::identity::{doc, Did};
use crate::storage::git::Repository;

const ASTERISK: char = '*';

static REFS_RAD: Lazy<RefString> = Lazy::new(|| refname!("refs/rad"));

/// Private trait to ensure that not any `Rule` can be deserialized.
/// Implementations are provided for `Allowed` and `usize` so that `RawRule`s
/// can be deserialized, while `ValidRule`s cannot – preventing deserialization
/// bugs for that type.
trait Sealed {}
impl Sealed for Allowed {}
impl Sealed for usize {}

/// A `Pattern` is a `QualifiedPattern` reference, however, it disallows any
/// references under the `refs/rad` hierarchy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "QualifiedPattern", try_from = "QualifiedPattern")]
pub struct Pattern(QualifiedPattern<'static>);

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl From<Pattern> for QualifiedPattern<'static> {
    fn from(Pattern(pattern): Pattern) -> Self {
        pattern
    }
}

impl<'a> TryFrom<QualifiedPattern<'a>> for Pattern {
    type Error = PatternError;

    fn try_from(pattern: QualifiedPattern<'a>) -> Result<Self, Self::Error> {
        if pattern.starts_with(REFS_RAD.as_str()) {
            Err(PatternError::ProtectedRef {
                prefix: (*REFS_RAD).clone(),
                pattern: pattern.to_owned(),
            })
        } else {
            Ok(Self(pattern.to_owned()))
        }
    }
}

impl<'a> TryFrom<Qualified<'a>> for Pattern {
    type Error = PatternError;

    fn try_from(name: Qualified<'a>) -> Result<Self, Self::Error> {
        Self::try_from(QualifiedPattern::from(name))
    }
}

impl Pattern {
    /// Check if the `refname` matches the rule's `refspec`.
    pub fn matches(&self, refname: &Qualified) -> bool {
        // N.b. Git's refspecs do not quite match with glob-star semantics. A
        // single `*` in a refspec is expected to match all references under
        // that namespace, even if they are further down the hierarchy.
        // Thus, the following rules are applied:
        //
        //   - a trailing `*` changes to `**/*`
        //   - a `*` in between path components changes to `**`
        let spec = match self.0.as_str().split_once(ASTERISK) {
            None => self.0.to_string(),
            // Expand `refs/tags/*` to `refs/tags/**/*`
            Some((prefix, "")) => {
                let mut spec = prefix.to_string();
                spec.push_str("**/*");
                spec
            }
            // Expand `refs/tags/*/v1.0` to `refs/tags/**/v1.0`
            Some((prefix, suffix)) => {
                let mut spec = prefix.to_string();
                spec.push_str("**");
                spec.push_str(suffix);
                spec
            }
        };
        fast_glob::glob_match(&spec, refname.as_str())
    }
}

impl AsRef<QualifiedPattern<'static>> for Pattern {
    fn as_ref(&self) -> &QualifiedPattern<'static> {
        &self.0
    }
}

impl PartialOrd for Pattern {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Patterns are ordered by their specificity.
///
/// This is heavily influenced by the evaluation priority of Rules. For a
/// candidate reference name, we want the rule associated with the most specific
/// pattern to apply, i.e. to take priority over all other rules with less
/// specific patterns.
///
/// For two patterns φ and ψ, we say that "φ is more specific than ψ", denoted
/// φ < ψ if:
///
///  1. The number of components in φ is larger than the number of components
///     in ψ. (Note that the number of components is equal to the number of
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
///     Note that this would also apply when the connection between the φ and ψ
///     is not as obvious, e.g. also "a/b/c/d/*" < "*/x".
///
/// (Note that for the following items, one may assume that φ and ψ have the
/// same number of components.)
///
///  2. If path component i of φ, denoted φ[i], is more specific than path
///     component i of ψ, denoted ψ[i]. This is the case if:
///      a. φ[i] does not contain an asterisk and ψ[i] contains an asterisk,
///         i.e. the symbol '*', e.g. "a" < "*" and "abc" < "a*".
///         Note that this is important to capture specificity accross
///         components, i.e. to conclude that "a/b/*" < "a/*/c".
///      b. Both φ[i] and ψ[i] contain an asterisk.
///          A. The asterisk in φ[i] is further right than the asterisk in φ[i],
///             e.g. "xx*" < "a*".
///          B. The asterisk in φ[i] and ψ[i] is equally far to the right,
///             and φ[i] is longer than ψ[i], e.g. "a*b" < "x*".
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
        let mut lhs = self.0.components();
        let mut rhs = other.0.components();

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

        use git::refspec::Component;

        fn cmp_component(lhs: Component<'_>, rhs: Component<'_>) -> ComponentOrdering {
            let (l, r) = (lhs.as_str(), rhs.as_str());
            match (l.find(ASTERISK), r.find(ASTERISK)) {
                (Some(_), None) => ComponentOrdering::MatchLength(Ordering::Greater), // (2.a.)
                (None, Some(_)) => ComponentOrdering::MatchLength(Ordering::Less),    // (2.a.)
                (Some(li), Some(ri)) => {
                    if li != ri {
                        ComponentOrdering::MatchLength(li.cmp(&ri).reverse()) // (2.b.A)
                    } else if l.len() != r.len() {
                        ComponentOrdering::MatchLength(l.len().cmp(&r.len()).reverse())
                    // (2.b.B)
                    } else {
                        ComponentOrdering::Lexicographic(l.cmp(r)) // (3.)
                    }
                }
                (None, None) => ComponentOrdering::Lexicographic(l.cmp(r)), // (3.)
            }
        }

        let mut result = ComponentOrdering::default();

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

/// A [`Rule`] that can be serialized and deserialized safely.
///
/// Should be converted to a [`ValidRule`] via [`Rule::validate`].
pub type RawRule = Rule<Allowed, usize>;

impl RawRule {
    /// Validate the `Rule` into a form that can be used for calculating
    /// canonical references.
    ///
    /// The `resolve` function is used to get the set of DIDs by inspecting the
    /// [`Allowed`] value. In most cases, if it is [`Allowed::Delegates`] then
    /// the closure will resolve the DIDs from the identity document, and if it
    /// is [`Allowed::Set`] it will validate the set.
    pub fn validate<R>(self, resolve: &mut R) -> Result<ValidRule, ValidationError>
    where
        R: Fn(Allowed) -> Result<doc::Delegates, ValidationError>,
    {
        let Self {
            allow: delegates,
            threshold,
            ..
        } = self;
        let allow = match &delegates {
            Allowed::Delegates => ResolvedDelegates::Delegates(resolve(delegates)?),
            Allowed::Set(_) => ResolvedDelegates::Set(resolve(delegates)?),
        };
        let threshold = doc::Threshold::new(threshold, &allow)?;
        Ok(Rule {
            allow,
            threshold,
            extensions: self.extensions,
        })
    }
}

/// A set of `RawRule`s that can be serialized and deserialized.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawRules {
    /// The reference pattern that this rule applies to.
    ///
    /// Note that this can be a fully-qualified pattern, e.g. `refs/heads/qa`,
    /// as well as a wild-card pattern, e.g. `refs/tags/*`.
    #[serde(flatten)]
    pub rules: BTreeMap<Pattern, RawRule>,
}

impl RawRules {
    /// Returns an iterator over the [`Pattern`] and [`RawRule`] in the set of
    /// rules.
    pub fn iter(&self) -> impl Iterator<Item = (&Pattern, &RawRule)> {
        self.rules.iter()
    }

    /// Add a new [`RawRule`] to the set of rules.
    ///
    /// Returns the replaced rule, if it existed.
    pub fn insert(&mut self, pattern: Pattern, rule: RawRule) -> Option<RawRule> {
        self.rules.insert(pattern, rule)
    }

    /// Remove the rule that matches the `pattern` parameter.
    ///
    /// Returns the rule if it existed.
    pub fn remove(&mut self, pattern: &Pattern) -> Option<RawRule> {
        self.rules.remove(pattern)
    }

    /// Check to see if there is an exact match for `refname` in the rules.
    pub fn exact_match(&self, refname: &Qualified) -> bool {
        let refname = refname.as_str();
        self.rules
            .iter()
            .any(|(pattern, _)| pattern.0.as_str() == refname)
    }

    /// Check if the `refname` matches any existing rules, including glob
    /// matches.
    pub fn matches(&self, refname: &Qualified) -> bool {
        self.rules
            .iter()
            .any(|(pattern, _)| pattern.matches(refname))
    }
}

impl Extend<(Pattern, RawRule)> for RawRules {
    fn extend<T: IntoIterator<Item = (Pattern, RawRule)>>(&mut self, iter: T) {
        self.rules.extend(iter)
    }
}

impl From<BTreeMap<Pattern, RawRule>> for RawRules {
    fn from(rules: BTreeMap<Pattern, RawRule>) -> Self {
        RawRules { rules }
    }
}

impl FromIterator<(Pattern, RawRule)> for RawRules {
    fn from_iter<T: IntoIterator<Item = (Pattern, RawRule)>>(iter: T) -> Self {
        iter.into_iter().collect::<BTreeMap<_, _>>().into()
    }
}

impl IntoIterator for RawRules {
    type Item = (Pattern, RawRule);
    type IntoIter = std::collections::btree_map::IntoIter<Pattern, RawRule>;

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

impl ValidRule {
    /// Initialize a `ValidRule` for the default branch, given by `name`. The
    /// rule will contain the single `did` as the allowed DID, and use a
    /// threshold of `1`.
    ///
    /// Note that the serialization of the rule will use the `delegates` token
    /// for the rule. E.g.
    /// ```json, no_run
    /// {
    ///   "pattern": "refs/heads/main",
    ///   "allow": ["did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"],
    ///   "threshold": 1
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// If the `name` reference begins with `refs/rad`.
    pub fn default_branch(did: Did, name: &git::RefStr) -> Result<(Pattern, Self), PatternError> {
        let pattern = Pattern::try_from(git::refs::branch(name).to_owned())?;
        let rule = Self {
            allow: ResolvedDelegates::Delegates(doc::Delegates::from(did)),
            // N.B. this needs to be the minimum since we only have one
            // delegate.
            threshold: doc::Threshold::MIN,
            extensions: json::Map::new(),
        };
        Ok((pattern, rule))
    }
}

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
    /// Explicit set of allowed DIDs.
    ///
    /// # Validation
    ///
    /// The set of allowed DIDs must be:
    ///   - Unique
    ///   - `1 <= delegates.len() <= 255`
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
    pub fn refname(&self) -> &Qualified {
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

    /// Return the [`Canonical`] representation for the matched rule.
    pub fn canonical(&self, repo: &Repository) -> Result<Canonical, git::raw::Error> {
        Canonical::reference(
            repo,
            &self.refname,
            self.rule.allow.as_ref(),
            self.rule.threshold.into(),
        )
    }
}

/// A set of valid [`Rule`]s, where the set of DIDs and threshold are fully
/// resolved and valid. Since the rules are constructed via a `BTreeMap`, they
/// cannot be duplicated.
///
/// To construct the set of rules, use [`Rules::from_raw`], which validates a
/// set of [`RawRule`]s, and their [`Pattern`] references, into a set of
/// [`ValidRule`]s.
///
/// The `Rules` can then be used to construct a [`MatchedRule`] by providing a
/// [`Qualified`] reference to see if it matches against any of the rules.
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

impl<'a> IntoIterator for &'a Rules {
    type Item = (&'a Pattern, &'a ValidRule);
    type IntoIter = std::collections::btree_map::Iter<'a, Pattern, ValidRule>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules.iter()
    }
}

impl IntoIterator for Rules {
    type Item = (Pattern, ValidRule);
    type IntoIter = std::collections::btree_map::IntoIter<Pattern, ValidRule>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules.into_iter()
    }
}

impl From<Rules> for RawRules {
    fn from(Rules { rules }: Rules) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|(pattern, rule)| (pattern, rule.into()))
                .collect(),
        }
    }
}

impl Rules {
    /// Returns an iterator over the [`Pattern`] and [`ValidRule`] in the set of
    /// rules.
    pub fn iter(&self) -> impl Iterator<Item = (&Pattern, &ValidRule)> {
        self.rules.iter()
    }

    /// Returns `true` is the set of rules is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Construct a set of `Rules` given a set of `RawRule`s.
    pub fn from_raw<R>(
        rules: impl IntoIterator<Item = (Pattern, RawRule)>,
        resolve: &mut R,
    ) -> Result<Self, ValidationError>
    where
        R: Fn(Allowed) -> Result<doc::Delegates, ValidationError>,
    {
        let valid = rules
            .into_iter()
            .map(|(pattern, rule)| rule.validate(resolve).map(|rule| (pattern, rule)))
            .collect::<Result<_, _>>()?;
        Ok(Self { rules: valid })
    }

    /// Return the first matching rule for the given `refname`, if there is any.
    ///
    /// N.b. it will find the first rule that is most specific for the given
    /// `refname`.
    pub fn matches<'a>(&self, refname: Qualified<'a>) -> Option<MatchedRule<'a>> {
        self.rules
            .iter()
            .find(|(pattern, _)| pattern.matches(&refname))
            .map(|(_, rule)| MatchedRule {
                refname,
                rule: rule.clone(),
            })
    }
}

/// A `Rule` defines how a reference or set of references can be made canonical,
/// i.e. have a top-level `refs/*` entry – see [`Pattern`].
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
    /// The set of delegates that are considered for voting for this rule.
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
    /// Construct a new `Rule` with the given `refspec`, `delegates`, and
    /// `threshold`.
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
pub enum PatternError {
    #[error("cannot create rule for '{pattern}' since references under '{prefix}' are reserved")]
    ProtectedRef {
        prefix: RefString,
        pattern: QualifiedPattern<'static>,
    },
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error(transparent)]
    Threshold(#[from] doc::ThresholdError),
    #[error(transparent)]
    Delegates(#[from] doc::DelegatesError),
    #[error("cannot create rule for reserved `rad` references '{pattern}'")]
    RadRef { pattern: QualifiedPattern<'static> },
}

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    References(#[from] git::ext::Error),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use nonempty::nonempty;

    use crate::crypto::{test::signer::MockSigner, Signer};
    use crate::git;
    use crate::git::refspec::qualified_pattern;
    use crate::git::RefString;
    use crate::identity::doc::Doc;
    use crate::identity::Visibility;
    use crate::rad;
    use crate::storage::refs::{IDENTITY_BRANCH, IDENTITY_ROOT, SIGREFS_BRANCH};
    use crate::storage::{git::transport, ReadStorage};
    use crate::test::{arbitrary, fixtures};
    use crate::Storage;

    use super::*;

    fn roundtrip(rule: &Rule<Allowed, usize>) {
        let json = serde_json::to_string(rule).unwrap();
        assert_eq!(
            *rule,
            serde_json::from_str(&json).unwrap(),
            "failed to roundtrip: {json}"
        )
    }

    fn did(s: &str) -> Did {
        s.parse().unwrap()
    }

    fn pattern(qp: QualifiedPattern<'static>) -> Pattern {
        Pattern::try_from(qp).unwrap()
    }

    fn resolve_from_doc(delegate: Allowed, doc: &Doc) -> Result<doc::Delegates, ValidationError> {
        match delegate {
            Allowed::Delegates => Ok(doc.delegates().clone()),
            Allowed::Set(delegates) => {
                doc::Delegates::new(delegates).map_err(ValidationError::from)
            }
        }
    }

    fn tag(name: RefString, head: git2::Oid, repo: &git2::Repository) -> git::Oid {
        let commit = fixtures::commit(name.as_str(), &[head], repo);
        let target = repo.find_object(*commit, None).unwrap();
        let tagger = repo.signature().unwrap();
        repo.tag(name.as_str(), &target, &tagger, name.as_str(), false)
            .unwrap()
            .into()
    }

    #[test]
    fn test_roundtrip() {
        let rule1 = Rule::new(Allowed::Delegates, 1);
        let rule2 = Rule::new(Allowed::Delegates, 1);
        let rule3 = Rule::new(Allowed::Delegates, 1);
        let mut rule4 = Rule::new(
            Allowed::Set(nonempty![
                did("did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K"),
                did("did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"),
            ]),
            2,
        );
        rule4.add_extensions(
            serde_json::json!({
                "foo": "bar",
                "quux": 5,
            })
            .as_object()
            .cloned()
            .unwrap(),
        );
        roundtrip(&rule1);
        roundtrip(&rule2);
        roundtrip(&rule3);
        roundtrip(&rule4);
    }

    #[test]
    fn test_deserialization() {
        let examples = r#"
{
  "refs/heads/main": {
    "threshold": 2,
    "allow": [
      "did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K",
      "did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"
    ]
  },
  "refs/tags/releases/*": {
    "threshold": 2,
    "allow": [
      "did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56",
      "did:key:z6Mkq2E5Se5H9gk1DsL1EMwR2t4CqSg3GFkNN2UeG4FNqXoP",
      "did:key:z6MkqRmXW5fbP9hJ1Y8j2N4CgVdJ2XJ6TsyXYf3FQ2NJgXax"
    ]
  },
  "refs/heads/development": {
    "threshold": 1,
    "allow": [
      "did:key:z6MkhH7ENYE62JAjTiRZPU71MGZ6xCwnbyHHWfrBu3fr6PVG"
    ]
  },
  "refs/heads/release/*": {
    "threshold": 1,
    "allow": "delegates"
  }
}
 "#;
        let expected = [
            (
                pattern(qualified_pattern!("refs/heads/main")),
                Rule::new(
                    Allowed::Set(nonempty![
                        did("did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K"),
                        did("did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"),
                    ]),
                    2,
                ),
            ),
            (
                pattern(qualified_pattern!("refs/tags/releases/*")),
                Rule::new(
                    Allowed::Set(nonempty![
                        did("did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56"),
                        did("did:key:z6Mkq2E5Se5H9gk1DsL1EMwR2t4CqSg3GFkNN2UeG4FNqXoP"),
                        did("did:key:z6MkqRmXW5fbP9hJ1Y8j2N4CgVdJ2XJ6TsyXYf3FQ2NJgXax")
                    ]),
                    2,
                ),
            ),
            (
                pattern(qualified_pattern!("refs/heads/development")),
                Rule::new(
                    Allowed::Set(nonempty![did(
                        "did:key:z6MkhH7ENYE62JAjTiRZPU71MGZ6xCwnbyHHWfrBu3fr6PVG"
                    )]),
                    1,
                ),
            ),
            (
                pattern(qualified_pattern!("refs/heads/release/*")),
                Rule::new(Allowed::Delegates, 1),
            ),
        ]
        .into_iter()
        .collect::<RawRules>();
        let rules = serde_json::from_str::<BTreeMap<Pattern, RawRule>>(examples)
            .unwrap()
            .into();
        eprintln!(
            "RULES: {}",
            serde_json::to_string_pretty(&expected).unwrap()
        );
        assert_eq!(expected, rules)
    }

    #[test]
    fn test_order() {
        assert!(
            pattern(qualified_pattern!("a/b/c/d/*")) < pattern(qualified_pattern!("*/x")),
            "example 1"
        );
        assert!(
            pattern(qualified_pattern!("a")) < pattern(qualified_pattern!("*")),
            "example 2.a"
        );
        assert!(
            pattern(qualified_pattern!("abc")) < pattern(qualified_pattern!("a*")),
            "example 2.a"
        );
        assert!(
            pattern(qualified_pattern!("a/b/*")) < pattern(qualified_pattern!("a/*/c")),
            "example 2.a"
        );
        assert!(
            pattern(qualified_pattern!("xx*")) < pattern(qualified_pattern!("a*")),
            "example 2.b.A"
        );
        assert!(
            pattern(qualified_pattern!("a*b")) < pattern(qualified_pattern!("x*")),
            "example 2.b.B"
        );

        let pattern01 = pattern(qualified_pattern!("refs/tags/*"));
        let pattern02 = pattern(qualified_pattern!("refs/tags/v1"));
        let pattern04 = pattern(qualified_pattern!("refs/tags/v1.0.0"));
        let pattern03 = pattern(qualified_pattern!("refs/heads/main"));
        let pattern05 = pattern(qualified_pattern!("refs/tags/release/v1.0.0"));
        let pattern06 = pattern(qualified_pattern!("refs/tags/*/v1.0.0"));

        let pattern07 = pattern(qualified_pattern!("refs/tags/x*"));
        let pattern08 = pattern(qualified_pattern!("refs/tags/xx*"));

        let pattern09 = pattern(qualified_pattern!("refs/foos/*"));

        let pattern10 = pattern(qualified_pattern!("a"));
        let pattern11 = pattern(qualified_pattern!("b"));

        let pattern12 = pattern(qualified_pattern!("a/*"));
        let pattern13 = pattern(qualified_pattern!("b/*"));

        let pattern14 = pattern(qualified_pattern!("a/*/ab"));
        let pattern15 = pattern(qualified_pattern!("a/*/a"));

        let pattern16 = pattern(qualified_pattern!("a/*/b"));
        let pattern17 = pattern(qualified_pattern!("a/*/a"));

        // Test priority for path specificity
        assert!(
            pattern06 < pattern02,
            "match for 06 is always more specific since it has more components"
        );
        assert!(pattern02 < pattern01, "match for 02 is also match for 01");
        assert!(pattern08 < pattern07, "match for 08 is also match for 07");
        // Test equality
        assert!(pattern02 == pattern02);
        // Test lexicographical fallback when paths are equally specific
        assert!(pattern02 < pattern04);
        assert!(pattern03 < pattern01);
        assert!(pattern09 < pattern01);
        assert!(pattern10 < pattern11);
        assert!(pattern12 < pattern13);
        assert!(pattern15 < pattern14);
        assert!(
            pattern17 < pattern16,
            "matches have same length, but lexicographically, 'a' < 'b'"
        );

        // Test example from docs
        let pattern18 = pattern(qualified_pattern!("refs/tags/release/candidates/*"));
        let pattern19 = pattern(qualified_pattern!("refs/tags/release/*"));
        let pattern20 = pattern(qualified_pattern!("refs/tags/*"));

        assert!(pattern18 < pattern19);
        assert!(pattern19 < pattern20);

        let pattern21 = pattern(qualified_pattern!("refs/heads/dev"));

        assert!(pattern21 < pattern03);

        let mut patterns = [
            pattern01.clone(),
            pattern02.clone(),
            pattern03.clone(),
            pattern04.clone(),
            pattern05.clone(),
            pattern06.clone(),
        ];
        patterns.sort();

        assert_eq!(
            patterns,
            [pattern05, pattern06, pattern03, pattern02, pattern04, pattern01]
        );
    }

    #[test]
    fn test_deserialize_extensions() {
        let example = r#"
{
  "threshold": 2,
  "allow": [
    "did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K",
    "did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"
  ],
  "foo": "bar",
  "quux": 5
}
"#;
        let rule = serde_json::from_str::<Rule<Allowed, usize>>(example).unwrap();
        assert!(!rule.extensions().is_empty());
        let extensions = rule.extensions();
        assert_eq!(
            extensions.get("foo"),
            Some(serde_json::Value::String("bar".to_string())).as_ref()
        );
        assert_eq!(
            extensions.get("quux"),
            Some(serde_json::Value::Number(5.into())).as_ref()
        );
    }

    #[test]
    fn test_rule_validate_success() {
        let doc = arbitrary::gen::<Doc>(1);
        let delegates = Allowed::Set(doc.delegates().as_ref().clone());
        let threshold = doc.majority();

        let rule = Rule::new(delegates, threshold);
        let result = rule.validate(&mut |delegate| resolve_from_doc(delegate, &doc));
        assert!(result.is_ok(), "failed to validate doc: {result:?}");

        let rule = Rule::new(Allowed::Delegates, 1);
        let result = rule.validate(&mut |delegate| resolve_from_doc(delegate, &doc));
        assert!(result.is_ok(), "failed to validate doc: {result:?}");
    }

    #[test]
    fn test_rule_validate_failures() {
        let doc = arbitrary::gen::<Doc>(1);
        let pattern = pattern(qualified_pattern!("refs/heads/main"));

        assert!(matches!(
            Rule::new(Allowed::Delegates, 256)
                .validate(&mut |delegate| resolve_from_doc(delegate, &doc)),
            Err(ValidationError::Threshold(_))
        ));

        let threshold = doc.delegates().len() + 1;
        assert!(matches!(
            Rule::new(Allowed::Delegates, threshold)
                .validate(&mut |delegate| resolve_from_doc(delegate, &doc)),
            Err(ValidationError::Threshold(_))
        ));

        let delegates = NonEmpty::from_vec(arbitrary::vec::<Did>(256)).unwrap();
        assert!(matches!(
            Rule::new(delegates.into(), 1)
                .validate(&mut |delegate| resolve_from_doc(delegate, &doc)),
            Err(ValidationError::Delegates(_))
        ));

        let delegates = nonempty![
            did("did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56"),
            did("did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56")
        ];
        let expected = Rule {
            allow: ResolvedDelegates::Set(
                doc::Delegates::new(nonempty![did(
                    "did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56"
                )])
                .unwrap(),
            ),
            threshold: doc::Threshold::MIN,
            extensions: json::Map::new(),
        };
        assert_eq!(
            Rule::new(delegates.into(), 1)
                .validate(&mut |delegate| resolve_from_doc(delegate, &doc))
                .unwrap(),
            expected,
        );

        // Duplicate rules are overwritten
        let rules = vec![
            (pattern.clone(), Rule::new(Allowed::Delegates, 1)),
            (
                pattern.clone(),
                Rule::new(doc.delegates().as_ref().clone().into(), 1),
            ),
        ];
        let expected = [(
            pattern,
            Rule::new(
                ResolvedDelegates::Set(doc.delegates().clone()),
                doc::Threshold::MIN,
            ),
        )]
        .into_iter()
        .collect::<Rules>();
        assert_eq!(
            Rules::from_raw(rules, &mut |delegate| resolve_from_doc(delegate, &doc)).unwrap(),
            expected
        );
    }

    #[test]
    fn test_canonical() {
        let tempdir = tempfile::tempdir().unwrap();
        let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        let delegate = MockSigner::from_seed([0xff; 32]);
        let contributor = MockSigner::from_seed([0xfe; 32]);
        let (repo, head) = fixtures::repository(tempdir.path().join("working"));
        let (rid, doc, _) = rad::init(
            &repo,
            "heartwood".try_into().unwrap(),
            "Radicle Heartwood Protocol & Stack",
            git::refname!("master"),
            Visibility::default(),
            &delegate,
            &storage,
        )
        .unwrap();

        let mut doc = doc.edit();
        // Ensure there is a second delegate for testing overlapping rules
        doc.delegate(contributor.public_key().into());

        // Create tags and keep track of their OIDs
        //
        // follows the `refs/tags/release/candidates/*` rule
        let failing_tag = git::refname!("release/candidates/v1.0");
        let tags = [
            // follows the `refs/tags/*` rule
            git::refname!("v1.0"),
            // follows the `refs/tags/release/*` rule
            git::refname!("release/v1.0"),
            failing_tag.clone(),
            // follows the `refs/tags/*` rule
            git::refname!("qa/v1.0"),
        ]
        .into_iter()
        .map(|name| {
            (
                git::lit::refs_tags(name.clone()).into(),
                tag(name, head, &repo),
            )
        })
        .collect::<BTreeMap<Qualified, _>>();

        git::push(
            &repo,
            &rad::REMOTE_NAME,
            [
                (
                    &git::qualified!("refs/tags/v1.0"),
                    &git::qualified!("refs/tags/v1.0"),
                ),
                (
                    &git::qualified!("refs/tags/release/v1.0"),
                    &git::qualified!("refs/tags/release/v1.0"),
                ),
                (
                    &git::qualified!("refs/tags/release/candidates/v1.0"),
                    &git::qualified!("refs/tags/release/candidates/v1.0"),
                ),
                (
                    &git::qualified!("refs/tags/qa/v1.0"),
                    &git::qualified!("refs/tags/qa/v1.0"),
                ),
            ],
        )
        .unwrap();

        let rules = Rules::from_raw(
            [
                (
                    pattern(qualified_pattern!("refs/tags/*")),
                    Rule::new(Allowed::Delegates, 1),
                ),
                (
                    pattern(qualified_pattern!("refs/tags/release/*")),
                    Rule::new(Allowed::Delegates, 1),
                ),
                // Ensure that none of the other rules apply by ensuring we need
                // both delegates to get the quorum of the
                // `refs/tags/release/candidates/v1.0` reference
                (
                    pattern(qualified_pattern!("refs/tags/release/candidates/*")),
                    Rule::new(Allowed::Delegates, 2),
                ),
            ],
            &mut |delegate| resolve_from_doc(delegate, &doc.clone().verified().unwrap()),
        )
        .unwrap();

        // All tags should succeed at getting their canonical tip other than the
        // candidates tag.
        let stored = storage.repository(rid).unwrap();
        let failing = git::Qualified::from(git::lit::refs_tags(failing_tag));
        for (refname, oid) in tags.iter() {
            let matched = rules.matches(refname.clone()).unwrap_or_else(|| {
                panic!("there should be a matching rule for {refname}, rules: {rules:#?}")
            });
            let canonical = matched.canonical(&stored).unwrap();
            if *refname == failing {
                assert!(canonical.quorum(&repo).is_err());
            } else {
                assert_eq!(
                    canonical
                        .quorum(&repo)
                        .unwrap_or_else(|e| panic!("quorum error for {refname}: {e}")),
                    *oid,
                )
            }
        }
    }

    #[test]
    fn test_special_branches() {
        assert!(Pattern::try_from((*IDENTITY_BRANCH).clone()).is_err());
        assert!(Pattern::try_from((*SIGREFS_BRANCH).clone()).is_err());
        assert!(Pattern::try_from((*IDENTITY_ROOT).clone()).is_err());
    }
}

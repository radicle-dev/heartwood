//! Symbolic references, which link neither to nor from protected references.
//! The prototypical example is `HEAD → refs/heads/main`.

use std::collections::{BTreeMap, BTreeSet};

use indexmap::IndexSet;
use serde::Serialize;

use crate::git::fmt::Qualified;
use crate::git::fmt::RefStr;
use crate::git::fmt::RefString;

use super::protect::{self, Unprotected};

/// A type alias for a [`RefString`] that has yet to be validated into a
/// a symbolic reference name.
pub type RawName = RefString;

/// A type alias for a [`RefString`] that has yet to be validated into a
/// symbolic reference target.
pub type RawTarget = RefString;

/// A freely mutable set of symbolic references which has not been validated.
pub type RawSymbolicRefs = BTreeMap<RawName, RawTarget>;

/// The target of a symbolic reference.
///
/// A target is either:
/// - [`Direct`](Target::Direct): a concrete qualified reference
///   (e.g. `refs/heads/main`).
/// - [`Symbolic`](Target::Symbolic): another symbolic reference name
///   (e.g. `MAIN`) that must itself resolve through the chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A concrete qualified reference — the end of a chain.
    Direct(Direct),
    /// Another symbolic reference name — an intermediate link.
    Symbolic(Symbolic),
}

impl AsRef<RefStr> for Target {
    fn as_ref(&self) -> &RefStr {
        match self {
            Target::Direct(direct) => direct.0.as_ref(),
            Target::Symbolic(symbolic) => symbolic.0.as_ref(),
        }
    }
}

/// A concrete qualified reference — the end of a chain.
// `Unprotected` has `super` visibility, so `Direct` is used to allow `Target`
// to be `pub`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Direct(Unprotected<Qualified<'static>>);

impl PartialEq<RefString> for Direct {
    fn eq(&self, other: &RefString) -> bool {
        **self.0.as_ref() == **other
    }
}

impl AsRef<Qualified<'static>> for Direct {
    fn as_ref(&self) -> &Qualified<'static> {
        self.0.as_ref()
    }
}

/// A concrete qualified reference — the end of a chain.
// `Unprotected` has `super` visibility, so `Symbolic` is used to allow `Target`
// to be `pub`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbolic(Unprotected<RefString>);

impl AsRef<RefString> for Symbolic {
    fn as_ref(&self) -> &RefString {
        self.0.as_ref()
    }
}

impl Target {
    /// Returns the underlying reference as a `&RefStr`.
    pub fn as_refstr(&self) -> &RefStr {
        match self {
            Target::Direct(q) => q.as_ref().as_ref(),
            Target::Symbolic(s) => s.as_ref().as_ref(),
        }
    }

    fn direct(d: Unprotected<Qualified<'static>>) -> Self {
        Self::Direct(Direct(d))
    }

    fn symbolic(s: Unprotected<RefString>) -> Self {
        Self::Symbolic(Symbolic(s))
    }

    /// Classify an [`Unprotected<RefString>`] as either
    /// [`Direct`](Target::Direct) or [`Symbolic`](Target::Symbolic)
    /// based on whether it is [`Qualified`].
    ///
    /// The [`Unprotected`] proof is preserved in the resulting variant.
    fn classify(s: Unprotected<RefString>) -> Self {
        match Qualified::from_refstr(s.as_ref()) {
            // Safety: the Qualified is derived from an Unprotected string,
            // so it is also unprotected.
            Some(q) => Target::direct(
                Unprotected::new(q.to_owned())
                    .expect("qualified derived from unprotected is unprotected"),
            ),
            None => Target::symbolic(s),
        }
    }
}

impl Serialize for Target {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_refstr().as_str())
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_refstr().as_str())
    }
}

/// Names of symbolic references are unprotected references.
/// Requiring [`Unprotected`] makes symbolic references that link *from*
/// protected references unrepresentable.
pub(super) type Name = Unprotected<RefString>;

impl std::cmp::Ord for Name {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl std::cmp::PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A validated, acyclic set of symbolic references.
///
/// Internally, targets are stored as [`Target`], which distinguishes
/// direct (qualified) targets from symbolic (intermediate) ones. This
/// means resolution and cycle-checking can pattern-match on the variant
/// rather than re-parsing the string.
///
/// Deserialize [`RawSymbolicRefs`] and convert it to this type to validate the
/// complete reference graph.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SymbolicRefs(BTreeMap<Name, Target>);

impl SymbolicRefs {
    /// Returns an iterator over all contained symbolic references, as pairs
    /// of their name and [`Target`].
    pub fn iter(&self) -> impl Iterator<Item = (&RawName, &Target)> {
        self.0.iter().map(|(name, target)| (name.as_ref(), target))
    }

    /// Returns an iterator over all contained symbolic references that
    /// resolve to a direct (qualified) target. The yielded target is the
    /// final [`Qualified`] reference after chasing through any intermediate
    /// symbolic references.
    pub fn iter_resolved(&self) -> impl Iterator<Item = (&RawName, &Qualified<'static>)> {
        self.0.keys().filter_map(|name| {
            self.resolve(name.as_ref())
                .map(|target| (name.as_ref(), target))
        })
    }

    /// Resolve a name through the chain of symbolic references until a
    /// [`Target::Direct`] target is reached. Returns `None` if the
    /// name is not in the map or if the chain dangles (ends at a
    /// [`Target::Symbolic`] whose name is not a key).
    pub fn resolve(&self, name: &RefString) -> Option<&Qualified<'static>> {
        let mut current = self.0.get(name)?;
        loop {
            match current {
                Target::Direct(q) => return Some(q.as_ref()),
                Target::Symbolic(s) => current = self.0.get(s.as_ref())?,
            }
        }
    }

    /// Returns `true` if the set of symbolic references is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Utilities for handling of `HEAD`.
impl SymbolicRefs {
    /// Convenience method to get the resolved target of the `HEAD` reference.
    /// Returns the final [`Qualified`] reference after chasing the chain.
    pub fn resolve_head(&self) -> Option<&Qualified<'static>> {
        self.resolve(Unprotected::head().as_ref())
    }
}

#[derive(Debug)]
pub enum ValidationError {
    Protected(protect::Error),
    Cycle(IndexSet<RefString>),
    TargetNotQualified { name: RawName, target: RawName },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Protected(err) => err.fmt(f),
            ValidationError::Cycle(cycle) => {
                write!(f, "symbolic references are cyclic: {:?}", cycle)
            }
            ValidationError::TargetNotQualified { name, target } => {
                write!(
                    f,
                    "symbolic reference '{name} → {target}' targets an unqualified reference"
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

impl TryFrom<RawSymbolicRefs> for SymbolicRefs {
    type Error = ValidationError;

    fn try_from(raw: RawSymbolicRefs) -> Result<Self, Self::Error> {
        let map = raw
            .into_iter()
            .map(|(name, target)| Ok((Unprotected::new(name)?, Unprotected::new(target)?)))
            .collect::<Result<BTreeMap<_, _>, protect::Error>>()
            .map_err(ValidationError::Protected)?;

        // Classify targets with knowledge of every key. In particular, a
        // qualified target is symbolic when that same string is also a key.
        let names: BTreeSet<_> = map.keys().cloned().collect();

        let entries = map
            .into_iter()
            .map(|(name, target)| {
                (
                    name,
                    if names.contains(target.as_ref()) {
                        Target::symbolic(target)
                    } else {
                        Target::classify(target)
                    },
                )
            })
            .collect();

        let result = Self(entries);

        // Validate every chain against the complete graph. Each chain must
        // terminate at a direct target and may not revisit a key.
        for (name, target) in &result.0 {
            let mut seen: IndexSet<&RefString> = IndexSet::from_iter([name.as_ref()]);
            let mut current = target;
            loop {
                match current {
                    Target::Direct(_) => break,
                    Target::Symbolic(symbolic) => {
                        let next = symbolic.as_ref();
                        if !seen.insert(next) {
                            return Err(ValidationError::Cycle(
                                seen.into_iter().cloned().collect(),
                            ));
                        }
                        current = result.0.get(next).ok_or_else(|| {
                            ValidationError::TargetNotQualified {
                                name: name.as_ref().clone(),
                                target: target.as_refstr().to_owned(),
                            }
                        })?;
                    }
                }
            }
        }

        Ok(result)
    }
}

impl From<SymbolicRefs> for RawSymbolicRefs {
    fn from(SymbolicRefs(refs): SymbolicRefs) -> Self {
        refs.into_iter()
            .map(|(name, target)| (name.into_inner(), target.as_refstr().to_owned()))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use crate::assert_matches;
    use crate::git::fmt::refname;

    use super::*;

    fn from_value(value: serde_json::Value) -> Result<SymbolicRefs, ValidationError> {
        SymbolicRefs::try_from(serde_json::from_value::<RawSymbolicRefs>(value).unwrap())
    }

    fn from_str(value: &str) -> Result<SymbolicRefs, ValidationError> {
        SymbolicRefs::try_from(serde_json::from_str::<RawSymbolicRefs>(value).unwrap())
    }

    fn from_pairs<const N: usize>(pairs: [(RawName, RawTarget); N]) -> SymbolicRefs {
        SymbolicRefs::try_from(RawSymbolicRefs::from(pairs)).unwrap()
    }

    #[test]
    fn infinite_single() {
        match SymbolicRefs::try_from(RawSymbolicRefs::from([(refname!("a"), refname!("a"))])) {
            Err(ValidationError::Cycle(cycle)) => {
                assert_eq!(cycle.len(), 1);
                assert_eq!(cycle.get_index(0), Some(&refname!("a")));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn infinite_multi() {
        match SymbolicRefs::try_from(RawSymbolicRefs::from([
            (refname!("a"), refname!("refs/heads/b")),
            (refname!("refs/heads/b"), refname!("refs/heads/c")),
            (refname!("refs/heads/c"), refname!("a")),
        ])) {
            Err(ValidationError::Cycle(cycle)) => {
                assert_eq!(cycle.len(), 3);
                assert_eq!(cycle.get_index(0), Some(&refname!("a")));
                assert_eq!(cycle.get_index(1), Some(&refname!("refs/heads/b")));
                assert_eq!(cycle.get_index(2), Some(&refname!("refs/heads/c")));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn deserialize_valid() {
        assert_matches!(
            from_value(serde_json::json!({
                "refs/heads/a": "refs/heads/b",
            })),
            Ok(_)
        );
    }

    #[test]
    fn deserialize_order() {
        assert_matches!(
            from_value(serde_json::json!({
                "MAIN": "refs/heads/master",
                "HEAD": "MAIN",
            })),
            Ok(_)
        );

        assert_matches!(
            from_value(serde_json::json!({
                "HEAD": "MAIN",
                "MAIN": "refs/heads/master",
            })),
            Ok(_)
        );
    }

    #[test]
    fn deserialize_infinite() {
        match from_value(serde_json::json!({
            "refs/heads/a": "refs/heads/a",
        })) {
            Err(ValidationError::Cycle(cycle)) => {
                assert_eq!(cycle.len(), 1);
                assert_eq!(cycle.get_index(0), Some(&refname!("refs/heads/a")));
            }
            _ => unreachable!(),
        }

        match from_value(serde_json::json!({
            "refs/heads/a": "refs/heads/b",
            "refs/heads/b": "refs/heads/c",
            "refs/heads/c": "refs/heads/a",
        })) {
            Err(ValidationError::Cycle(cycle)) => {
                assert_eq!(cycle.len(), 3);
                assert_eq!(cycle.get_index(0), Some(&refname!("refs/heads/a")));
                assert_eq!(cycle.get_index(1), Some(&refname!("refs/heads/b")));
                assert_eq!(cycle.get_index(2), Some(&refname!("refs/heads/c")));
            }
            _ => unreachable!(),
        }

        assert_matches!(
            from_value(serde_json::json!({
                "HEAD": "b",
            })),
            Err(ValidationError::TargetNotQualified { .. })
        );
    }

    #[test]
    fn raw_is_freely_mutable() {
        let mut raw = RawSymbolicRefs::default();
        raw.insert(refname!("a"), refname!("b"));
        raw.insert(refname!("b"), refname!("a"));

        assert_eq!(raw.len(), 2);

        match SymbolicRefs::try_from(raw) {
            Err(ValidationError::Cycle(cycle)) => {
                assert_eq!(cycle.len(), 2);
                assert_eq!(cycle.get_index(0), Some(&refname!("a")));
                assert_eq!(cycle.get_index(1), Some(&refname!("b")));
            }
            _ => unreachable!(),
        }
    }

    /// Verifies that resolution works correctly for chains with 2 links
    /// (even-length), e.g. `HEAD → MAIN → refs/heads/master`.
    #[test]
    fn resolve_two_hop_chain() {
        let symrefs = from_value(serde_json::json!({
            "MAIN": "refs/heads/master",
            "HEAD": "MAIN",
        }))
        .unwrap();

        // HEAD → MAIN → refs/heads/master should resolve to refs/heads/master
        assert_eq!(
            symrefs.resolve_head().map(|r| r.as_str()),
            Some("refs/heads/master"),
        );
    }

    /// Verifies that direct targets are stored as [`Target::Direct`].
    #[test]
    fn target_classification() {
        let symrefs = from_value(serde_json::json!({
            "HEAD": "refs/heads/main",
        }))
        .unwrap();

        let (_, target) = symrefs.iter().next().unwrap();
        assert_matches!(target, Target::Direct(_));
    }

    /// Verifies that symbolic targets are stored as [`Target::Symbolic`].
    #[test]
    fn target_classification_symbolic() {
        let symrefs = from_value(serde_json::json!({
            "MAIN": "refs/heads/master",
            "HEAD": "MAIN",
        }))
        .unwrap();

        let head_entry = symrefs
            .iter()
            .find_map(|(name, target)| (name.as_str() == "HEAD").then_some(target))
            .unwrap();
        assert_matches!(head_entry, Target::Symbolic(_));

        let main_entry = symrefs
            .iter()
            .find_map(|(name, target)| (name.as_str() == "MAIN").then_some(target))
            .unwrap();
        assert_matches!(main_entry, Target::Direct(_));
    }

    /// Verifies that an existing direct target can become a symbolic target
    /// during a new insertion.
    #[test]
    fn target_reclassification() {
        let symrefs = from_pairs([
            (refname!("HEAD"), refname!("refs/heads/main")),
            (refname!("refs/heads/main"), refname!("refs/heads/master")),
        ]);
        let main = symrefs
            .iter()
            .find_map(|(_, target)| {
                (target.as_refstr().as_str() == "refs/heads/main").then_some(target)
            })
            .unwrap();
        assert_matches!(main, Target::Symbolic(_));
    }

    /// Verifies that an existing direct target can become a symbolic target
    /// during a new insertion.
    #[test]
    fn target_reclassification_commutative() {
        let symrefs = from_pairs([
            (refname!("refs/heads/main"), refname!("refs/heads/master")),
            (refname!("HEAD"), refname!("refs/heads/main")),
        ]);
        let main = symrefs
            .iter()
            .find_map(|(_, target)| {
                (target.as_refstr().as_str() == "refs/heads/main").then_some(target)
            })
            .unwrap();
        assert_matches!(main, Target::Symbolic(_));
    }

    #[test]
    fn reclassification_reverse_chain() {
        // Build the chain in reverse: terminal first, origin last.
        let symrefs = from_pairs([
            (refname!("refs/heads/c"), refname!("refs/heads/d")),
            (refname!("refs/heads/b"), refname!("refs/heads/c")),
            (refname!("refs/heads/a"), refname!("refs/heads/b")),
        ]);

        // Only refs/heads/d (the terminal) should be Direct.
        // refs/heads/b and refs/heads/c are both keys AND targets — Symbolic.
        for (_, target) in symrefs.iter() {
            match target.as_refstr().as_str() {
                "refs/heads/d" => assert_matches!(target, Target::Direct(_)),
                other => {
                    assert_matches!(target, Target::Symbolic(_), "expected Symbolic for {other}")
                }
            }
        }

        // Resolution should still work through the full chain.
        assert_eq!(
            symrefs
                .resolve(&refname!("refs/heads/a"))
                .map(|q| q.as_str()),
            Some("refs/heads/d"),
        );
    }

    #[test]
    fn reclassification_diamond() {
        let symrefs = from_pairs([
            (refname!("HEAD"), refname!("refs/heads/main")),
            (refname!("DEFAULT"), refname!("refs/heads/main")),
            (refname!("refs/heads/main"), refname!("refs/heads/master")),
        ]);

        // Both HEAD and DEFAULT's targets should now be Symbolic.
        let targets_for_main: Vec<_> = symrefs
            .iter()
            .filter(|(_, t)| t.as_refstr().as_str() == "refs/heads/main")
            .collect();
        assert_eq!(targets_for_main.len(), 2);
        for (name, target) in targets_for_main {
            assert_matches!(
                target,
                Target::Symbolic(_),
                "expected Symbolic for {name}'s target"
            );
        }
    }

    #[test]
    fn reclassification_order_invariant() {
        // Order A: HEAD first, then the chain link.
        let a = from_pairs([
            (refname!("HEAD"), refname!("refs/heads/main")),
            (refname!("refs/heads/main"), refname!("refs/heads/master")),
        ]);

        // Order B: chain link first, then HEAD.
        let b = from_pairs([
            (refname!("refs/heads/main"), refname!("refs/heads/master")),
            (refname!("HEAD"), refname!("refs/heads/main")),
        ]);

        // Both should resolve HEAD to the same place.
        assert_eq!(a.resolve_head(), b.resolve_head());

        // Both should have the same classification for the refs/heads/main target.
        let classify_a = a
            .iter()
            .find(|(_, t)| t.as_refstr().as_str() == "refs/heads/main")
            .unwrap()
            .1;
        let classify_b = b
            .iter()
            .find(|(_, t)| t.as_refstr().as_str() == "refs/heads/main")
            .unwrap()
            .1;
        assert_matches!(classify_a, Target::Symbolic(_));
        assert_matches!(classify_b, Target::Symbolic(_));
    }

    #[test]
    fn reclassification_combine() {
        // A has HEAD → refs/heads/main (Direct)
        let mut raw = RawSymbolicRefs::from([(refname!("HEAD"), refname!("refs/heads/main"))]);

        // B has refs/heads/main → refs/heads/master (Direct)
        raw.insert(refname!("refs/heads/main"), refname!("refs/heads/master"));
        let a = SymbolicRefs::try_from(raw).unwrap();

        // After combine, HEAD's target refs/heads/main should be Symbolic.
        let main_target = a
            .iter()
            .find(|(_, t)| t.as_refstr().as_str() == "refs/heads/main")
            .unwrap()
            .1;
        assert_matches!(main_target, Target::Symbolic(_));
        assert_eq!(
            a.resolve_head().map(|q| q.as_str()),
            Some("refs/heads/master")
        );
    }

    #[test]
    fn reclassification_combine_reverse() {
        // B has refs/heads/main → refs/heads/master (Direct)
        let mut raw =
            RawSymbolicRefs::from([(refname!("refs/heads/main"), refname!("refs/heads/master"))]);

        // A has HEAD → refs/heads/main (Direct)
        raw.insert(refname!("HEAD"), refname!("refs/heads/main"));
        let b = SymbolicRefs::try_from(raw).unwrap();

        // HEAD's target refs/heads/main IS a key — should be Symbolic.
        let main_target = b
            .iter()
            .find_map(|(_, t)| (t.as_refstr().as_str() == "refs/heads/main").then_some(t))
            .unwrap();
        assert_matches!(main_target, Target::Symbolic(_));
        assert_eq!(
            b.resolve_head().map(|q| q.as_str()),
            Some("refs/heads/master")
        );
    }

    /// Validated symbolic references must survive a round trip through raw
    /// canonical JSON, which is how an identity document is written to storage
    /// (see [`crate::identity::Doc::encode`]).
    #[test]
    fn canonical_roundtrip() {
        use crate::canonical::formatter::CanonicalFormatter;

        let symrefs = from_value(serde_json::json!({
            "MAIN": "refs/heads/master",
            "HEAD": "MAIN",
        }))
        .unwrap();

        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, CanonicalFormatter::new());
        symrefs.serialize(&mut ser).unwrap();
        let canonical = String::from_utf8(buf).unwrap();

        assert_eq!(from_str(&canonical).unwrap(), symrefs,);
    }
}

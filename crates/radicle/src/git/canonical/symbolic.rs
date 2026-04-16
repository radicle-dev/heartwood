//! Symbolic references, which link neither to nor from protected references.
//! The prototypical example is `HEAD → refs/heads/main`.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git::fmt::Qualified;
use crate::git::fmt::RefStr;
use crate::git::fmt::RefString;

use super::protect::Unprotected;

/// A type alias for a [`RefString`] that has yet to be validated into a
/// a symbolic reference name.
pub type RawName = RefString;

/// A type alias for a [`RefString`] that has yet to be validated into a
/// symbolic reference target.
pub type RawTarget = RefString;

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

impl Direct {
    fn to_ref_string(&self) -> Unprotected<RefString> {
        self.0.to_ref_string()
    }
}

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

/// Maintains a cycle-free set of symbolic references.
/// Note that dangling references are not detected.
///
/// Internally, targets are stored as [`Target`], which distinguishes
/// direct (qualified) targets from symbolic (intermediate) ones. This
/// means resolution and cycle-checking can pattern-match on the variant
/// rather than re-parsing the string.
///
/// # Deserialization Order
///
/// Deserialization validates entries in iteration (insertion) order via
/// [`TryFrom<IndexMap>`]. This means the validity of a JSON object depends
/// on its key order: a symbolic reference whose target is another symbolic
/// reference must appear *after* that target in the JSON. For example,
/// `{"MAIN": "refs/heads/master", "HEAD": "MAIN"}` is valid, but
/// `{"HEAD": "MAIN", "MAIN": "refs/heads/master"}` is not.
///
/// While JSON objects are nominally unordered (RFC 8259 §4), `serde_json`
/// with `IndexMap` preserves insertion order. Any serializer producing
/// this JSON must preserve key order for valid round-tripping.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "IndexMap<Name, Unprotected<RefString>>")]
pub struct SymbolicRefs(IndexMap<Name, Target>);

/// Read-only access.
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
    fn resolve(&self, name: &RefString) -> Option<&Qualified<'static>> {
        let mut current = self.0.get(name)?;
        loop {
            match current {
                Target::Direct(q) => return Some(q.as_ref()),
                Target::Symbolic(s) => match self.0.get(s.as_ref()) {
                    Some(next) => current = next,
                    None => return None,
                },
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
    /// Construct [`SymbolicRefs`] for the single symbolic reference `HEAD`
    /// targeting `/refs/heads/<branch_name>`.
    // This exists to encapsulate [`Unprotected`].
    pub fn head(branch_name: &RefString) -> Self {
        let mut result = Self::default();
        result
            .try_insert_unprotected(
                Unprotected::head().to_owned(),
                Unprotected::branch(branch_name).to_ref_string(),
            )
            .expect("not creating cycle");
        result
    }

    /// Convenience method to get the resolved target of the `HEAD` reference.
    /// Returns the final [`Qualified`] reference after chasing the chain.
    /// See also [`SymbolicRefs::head`].
    pub fn resolve_head(&self) -> Option<&Qualified<'static>> {
        self.resolve(Unprotected::head().as_ref())
    }
}

#[derive(Debug, Error)]
pub enum InsertionError {
    #[error("inserting symbolic reference '{name} → {target}' would create a cycle")]
    Cyclic { name: RawName, target: RawName },

    #[error(
        "inserting symbolic reference '{name} → {target}' would result in a symbolic reference targeting an unqualified reference"
    )]
    TargetNotQualified { name: RawName, target: RawName },
}

/// Mutability.
impl SymbolicRefs {
    /// Insert a symbolic reference from the given `name` to the given `target`.
    ///
    /// Internally, this classifies the `target` into the [`Target`] and
    /// delegates the insertion.
    pub(super) fn try_insert_unprotected(
        &mut self,
        name: Name,
        target: Unprotected<RefString>,
    ) -> Result<(), InsertionError> {
        self.insert(name, Target::classify(target))
    }

    /// Check whether `name` is reachable from `start` by chasing through
    /// the map. Used to detect cycles before insertion.
    ///
    /// Unlike [`resolve`](SymbolicRefs::resolve), this chases through
    /// *all* entries regardless of whether the target is [`Direct`](Target::Direct)
    /// or [`Symbolic`](Target::Symbolic), since a qualified ref string can
    /// also be a key in the map and thus part of a cycle.
    fn is_reachable_from(&self, start: &RefString, name: &Name) -> bool {
        let name = name.as_ref();
        if start == name {
            return true;
        }
        let mut current: &RefStr = start;
        loop {
            match self.0.get(current) {
                None => return false,
                Some(target) => {
                    let next = target.as_refstr();
                    if *next == **name {
                        return true;
                    }
                    current = next;
                }
            }
        }
    }

    /// Try to insert a symbolic reference.
    /// Errors if `name` or `target` is protected (see [`protect`]) or would
    /// cause infinite recursion (e.g. `A → B` and `B → A`).
    ///
    /// # Panics
    ///
    /// If `name` or `target` is not unprotected.
    #[cfg(test)]
    fn try_insert(&mut self, name: RawName, target: RawTarget) -> Result<(), InsertionError> {
        self.try_insert_unprotected(
            Unprotected::new(name).expect("name is unprotected"),
            Unprotected::new(target).expect("target is unprotected"),
        )
    }

    /// Consume `other` by iteratively inserting into self.
    pub fn combine(&mut self, other: SymbolicRefs) -> Result<(), InsertionError> {
        for (name, target) in other.0 {
            self.insert(name, target)?;
        }
        Ok(())
    }

    /// Insert a symbolic reference from the given `name` to the given `target`.
    ///
    /// The targets in the map can change their classification from
    /// [`Target::Direct`] to [`Target::Symbolic`], if the new insertion of the
    /// `name` or `target` matches an existing key or existing entries.
    ///
    /// # Errors
    ///
    /// The `target` reference must either:
    /// - Be a direct [`Qualified`] reference, or
    /// - Resolve to a direct [`Qualified`] reference, if it is a keyed entry in [`SymbolicRefs`].
    ///
    /// If neither of these is satisfied then an
    /// [`InsertionError::TargetNotQualified`] error is returned.
    ///
    /// If the `name` and `target` end up in a cycle, e.g., `a → b → a`, then an
    /// [`InsertionError::Cyclic`] error is returned.
    fn insert(&mut self, name: Name, mut target: Target) -> Result<(), InsertionError> {
        let target_str = match &target {
            Target::Direct(q) => q.as_ref().to_ref_string(),
            Target::Symbolic(s) => s.as_ref().clone(),
        };

        if self.is_reachable_from(&target_str, &name) {
            return Err(InsertionError::Cyclic {
                name: name.into_inner(),
                target: target_str,
            });
        }

        // A Direct target whose string is already a key is actually
        // an intermediate link — downgrade to Symbolic.
        if let Target::Direct(q) = &target {
            if self.0.contains_key(&q.as_ref().to_ref_string()) {
                target = Target::symbolic(q.to_ref_string());
            }
        }

        if let Target::Symbolic(s) = &target {
            if self.resolve(s.as_ref()).is_none() {
                return Err(InsertionError::TargetNotQualified {
                    name: name.into_inner(),
                    target: target_str,
                });
            }
        }

        self.reclassify_targets(&name);
        self.0.insert(name, target);
        Ok(())
    }

    /// When a new key is inserted, any existing `Direct` target whose
    /// qualified string matches the new key is no longer terminal — it's
    /// now an intermediate link and must be reclassified as `Symbolic`.
    fn reclassify_targets(&mut self, new_key: &Name) {
        let key = new_key.as_ref();
        for existing in self.0.values_mut() {
            if let Target::Direct(q) = existing {
                if q == key {
                    *existing = Target::symbolic(q.to_ref_string());
                }
            }
        }
    }
}

impl TryFrom<IndexMap<Name, Unprotected<RefString>>> for SymbolicRefs {
    type Error = InsertionError;

    fn try_from(map: IndexMap<Name, Unprotected<RefString>>) -> Result<Self, Self::Error> {
        map.into_iter()
            .try_fold(Self::default(), |mut result, (name, target)| {
                result.try_insert_unprotected(name, target)?;
                Ok(result)
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use crate::assert_matches;
    use crate::git::fmt::refname;

    use super::*;

    #[test]
    fn infinite_single() {
        let mut symbolic = SymbolicRefs::default();

        assert_matches!(
            symbolic.try_insert(refname!("a"), refname!("a")),
            Err(InsertionError::Cyclic { .. })
        );

        assert!(symbolic.is_empty());
    }

    #[test]
    fn infinite_multi() {
        let mut symbolic = SymbolicRefs::default();

        assert_matches!(
            symbolic.try_insert(refname!("a"), refname!("refs/heads/b")),
            Ok(())
        );

        assert_matches!(
            symbolic.try_insert(refname!("refs/heads/b"), refname!("refs/heads/c")),
            Ok(())
        );

        assert_matches!(
            symbolic.try_insert(refname!("refs/heads/c"), refname!("a")),
            Err(InsertionError::Cyclic { .. })
        );
    }

    #[test]
    fn deserialize_valid() {
        assert_matches!(
            serde_json::from_value::<SymbolicRefs>(serde_json::json!({
                "refs/heads/a": "refs/heads/b",
            })),
            Ok(_)
        );
    }

    #[test]
    fn deserialize_order() {
        assert_matches!(
            serde_json::from_value::<SymbolicRefs>(serde_json::json!({
                "MAIN": "refs/heads/master",
                "HEAD": "MAIN",
            })),
            Ok(_)
        );

        assert_matches!(
            serde_json::from_value::<SymbolicRefs>(serde_json::json!({
                "HEAD": "MAIN",
                "MAIN": "refs/heads/master",
            })),
            Err(_)
        );
    }

    #[test]
    fn deserialize_infinite() {
        assert_matches!(
            serde_json::from_value::<SymbolicRefs>(serde_json::json!({
                "refs/heads/a": "refs/heads/a",
            })),
            Err(_)
        );

        assert_matches!(
            serde_json::from_value::<SymbolicRefs>(serde_json::json!({
                "refs/heads/a": "refs/heads/b",
                "refs/heads/b": "refs/heads/c",
                "refs/heads/c": "refs/heads/a",
            })),
            Err(_)
        );

        assert_matches!(
            serde_json::from_value::<SymbolicRefs>(serde_json::json!({
                "HEAD": "b",
            })),
            Err(_)
        );
    }

    /// Verifies that resolution works correctly for chains with 2 links
    /// (even-length), e.g. `HEAD → MAIN → refs/heads/master`.
    #[test]
    fn resolve_two_hop_chain() {
        let symrefs = serde_json::from_value::<SymbolicRefs>(serde_json::json!({
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

    /// Motivates why we cannot simply delegate to [`BTreeMap::extend`]
    /// for combining [`SymbolicRefs`].
    #[test]
    fn infinite_extend() {
        let mut a = SymbolicRefs::default();
        assert_matches!(
            a.try_insert(refname!("refs/heads/a"), refname!("refs/heads/b")),
            Ok(())
        );

        let mut b = SymbolicRefs::default();
        assert_matches!(
            b.try_insert(refname!("refs/heads/b"), refname!("refs/heads/a")),
            Ok(())
        );

        assert_matches!(a.combine(b), Err(InsertionError::Cyclic { .. }));
    }

    /// Verifies that direct targets are stored as [`Target::Direct`].
    #[test]
    fn target_classification() {
        let symrefs = serde_json::from_value::<SymbolicRefs>(serde_json::json!({
            "HEAD": "refs/heads/main",
        }))
        .unwrap();

        let (_, target) = symrefs.iter().next().unwrap();
        assert_matches!(target, Target::Direct(_));
    }

    /// Verifies that symbolic targets are stored as [`Target::Symbolic`].
    #[test]
    fn target_classification_symbolic() {
        let symrefs = serde_json::from_value::<SymbolicRefs>(serde_json::json!({
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
        let mut symrefs = SymbolicRefs::default();
        symrefs
            .try_insert(refname!("HEAD"), refname!("refs/heads/main"))
            .unwrap();
        symrefs
            .try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
            .unwrap();
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
        let mut symrefs = SymbolicRefs::default();
        symrefs
            .try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
            .unwrap();
        symrefs
            .try_insert(refname!("HEAD"), refname!("refs/heads/main"))
            .unwrap();
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
        let mut symrefs = SymbolicRefs::default();

        // Build the chain in reverse: terminal first, origin last.
        for (name, target) in [
            (refname!("refs/heads/c"), refname!("refs/heads/d")),
            (refname!("refs/heads/b"), refname!("refs/heads/c")),
            (refname!("refs/heads/a"), refname!("refs/heads/b")),
        ] {
            symrefs.try_insert(name, target).unwrap();
        }

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
        let mut symrefs = SymbolicRefs::default();
        symrefs
            .try_insert(refname!("HEAD"), refname!("refs/heads/main"))
            .unwrap();
        symrefs
            .try_insert(refname!("DEFAULT"), refname!("refs/heads/main"))
            .unwrap();

        // Both targets are Direct — refs/heads/main is not a key yet.
        // Now make it a key:
        symrefs
            .try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
            .unwrap();

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
        let a = {
            let mut s = SymbolicRefs::default();
            s.try_insert(refname!("HEAD"), refname!("refs/heads/main"))
                .unwrap();
            s.try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
                .unwrap();
            s
        };

        // Order B: chain link first, then HEAD.
        let b = {
            let mut s = SymbolicRefs::default();
            s.try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
                .unwrap();
            s.try_insert(refname!("HEAD"), refname!("refs/heads/main"))
                .unwrap();
            s
        };

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
        let mut a = SymbolicRefs::head(&refname!("main"));

        // B has refs/heads/main → refs/heads/master (Direct)
        let mut b = SymbolicRefs::default();
        b.try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
            .unwrap();

        a.combine(b).unwrap();

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
        let mut b = SymbolicRefs::default();
        b.try_insert(refname!("refs/heads/main"), refname!("refs/heads/master"))
            .unwrap();

        // A has HEAD → refs/heads/main (Direct)
        let a = SymbolicRefs::head(&refname!("main"));

        b.combine(a).unwrap();

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
}

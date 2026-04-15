//! Symbolic references, which link neither to nor from protected references.
//! The prototypical example is `HEAD → refs/heads/main`.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git::fmt::Qualified;
use crate::git::fmt::RefString;

use super::protect::Unprotected;

use reachability::reachable;

pub type RawName = RefString;

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

pub type RawTarget = RefString;

/// Targets for symbolic references are unprotected references.
/// Requiring [`Unprotected`] makes symbolic references that link *to*
/// protected references unrepresentable.
pub(super) type Target = Unprotected<RefString>;

/// Maintains a cycle-free set of symbolic references.
/// Note that dangling references are not detected.
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "IndexMap<Name, Target>")]
pub struct SymbolicRefs(IndexMap<Name, Target>);

/// Read-only access.
impl SymbolicRefs {
    /// Returns an iterator over all contained symbolic references, as pairs of
    /// their name [`RawName`] and [`RawTarget`].
    pub fn iter(&self) -> impl Iterator<Item = (&RawName, &RawTarget)> {
        self.0
            .iter()
            .map(|(name, target)| (name.as_ref(), target.as_ref()))
    }

    /// Returns an iterator over all contained symbolic references, as pairs of
    /// their name [`RawName`] and resolved [`RawTarget`].
    pub fn iter_resolved(&self) -> impl Iterator<Item = (&RawName, &RawTarget)> {
        self.iter_resolved_unprotected()
            .map(|(name, target)| (name.as_ref(), target.as_ref()))
    }

    pub(super) fn iter_resolved_unprotected(&self) -> impl Iterator<Item = (&Name, &Target)> {
        self.0
            .keys()
            .filter_map(|name| self.resolve_unprotected(name).map(|target| (name, target)))
    }

    fn resolve_unprotected<'a>(&'a self, name: &Name) -> Option<&'a Target> {
        let mut target = self.0.get(name)?;
        while let Some(next) = self.0.get(target) {
            target = next;
        }
        Some(target)
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

    /// Convenience method to get the target of the `HEAD` reference.
    /// See also [`SymbolicRefs::head`].
    pub fn resolve_head(&self) -> Option<&RawTarget> {
        self.resolve_unprotected(&Unprotected::head())
            .map(|target| target.as_ref())
    }
}

#[derive(Debug, Error)]
pub enum InsertionError {
    #[error("inserting symbolic reference '{name} → {target}' would create a cycle")]
    Cyclic { name: RawName, target: RawTarget },

    #[error(
        "inserting symbolic reference '{name} → {target}' would result in a symbolic reference targeting an unqualified reference"
    )]
    TargetNotQualified { name: RawName, target: RawTarget },
}

/// Mutability.
impl SymbolicRefs {
    /// Insert a symbolic reference.
    /// Even though this method will never return [`InsertionError::Protected`]
    /// we opt to return that (slightly more general) error, as it allows
    /// construction of [`InsertionError::Cyclic`] by consuming `name` and
    /// `target`, avoiding an early copy in [`Self::try_insert`].
    pub(super) fn try_insert_unprotected(
        &mut self,
        name: Name,
        target: Target,
    ) -> Result<(), InsertionError> {
        if reachable(&self.0, &target, &name) {
            return Err(InsertionError::Cyclic {
                name: name.into_inner(),
                target: target.into_inner(),
            });
        }

        let target_is_qualified = Qualified::from_refstr(target.as_ref()).is_some();

        if !target_is_qualified {
            match self.resolve_unprotected(&target) {
                Some(end) => {
                    if Qualified::from_refstr(end.as_ref()).is_none() {
                        return Err(InsertionError::TargetNotQualified {
                            name: name.into_inner(),
                            target: target.into_inner(),
                        });
                    }
                }
                None => {
                    return Err(InsertionError::TargetNotQualified {
                        name: name.into_inner(),
                        target: target.into_inner(),
                    });
                }
            }
        }

        self.0.insert(name, target);
        Ok(())
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
            self.try_insert_unprotected(name, target)?;
        }
        Ok(())
    }
}

impl TryFrom<IndexMap<Name, Target>> for SymbolicRefs {
    type Error = InsertionError;

    fn try_from(map: IndexMap<Name, Target>) -> Result<Self, Self::Error> {
        let mut result = Self::default();
        for (name, target) in map.iter() {
            result.try_insert_unprotected(name.clone(), target.clone())?;
        }
        Ok(result)
    }
}

mod reachability {
    pub(super) trait Get<'a, 'b, K, V> {
        fn get(&'a self, key: &'b K) -> Option<&'a V>;
    }

    impl<'a, 'b, K: Eq + std::hash::Hash, V> Get<'a, 'b, K, V> for indexmap::IndexMap<K, V> {
        fn get(&'a self, key: &'b K) -> Option<&'a V> {
            indexmap::IndexMap::get(self, key)
        }
    }

    /// A reachability check linking
    /// from `K` to `V` using [`Get`], and
    /// from `V` to `K` using [`Into`].
    /// Note that the bound is trivial if `K = V`.
    ///
    /// This can be used to check whether inserting `key → val`
    /// would create a cycle.
    ///
    /// # Returns
    ///
    /// Whether `key == val` (under [`Into::into`]) or
    /// `key` is reachable from `val` (under [`Into::into`] and [`Get::get`]).
    pub(super) fn reachable<'a, 'b, K: PartialEq, V: 'a>(
        map: &'a impl Get<'a, 'b, K, V>,
        val: &'b V,
        key: &'b K,
    ) -> bool
    where
        'a: 'b,
        &'b V: Into<&'b K>,
    {
        // Self-Reference
        let src = val.into();
        if *src == *key {
            return true;
        }

        // Chase
        let mut src = src;
        while let Some(tmp) = map.get(src).map(|value| value.into()) {
            if *tmp == *key {
                return true;
            }
            src = tmp;
        }

        false
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
}

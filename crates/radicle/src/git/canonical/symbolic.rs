//! Symbolic references, which link neither to nor from protected references.
//! The prototypical example is `HEAD → refs/heads/main`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git::fmt::RefString;

use super::protect::{self, Unprotected};

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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BTreeMap<Name, Target>")]
pub struct SymbolicRefs(BTreeMap<Name, Target>);

// Read-only access.
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

    fn resolve_unprotected<'a, 'b>(&'a self, mut name: &'b Name) -> Option<&'a Target>
    where
        'a: 'b,
    {
        while let Some(target) = self.0.get(name) {
            match self.0.get(target) {
                Some(next) => {
                    name = next;
                }
                None => return Some(target),
            }
        }

        None
    }

    /// Returns `true` if the set of symbolic references is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// Utilities for handling of `HEAD`.
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
    pub fn get_head(&self) -> Option<&RawTarget> {
        self.resolve_unprotected(&Unprotected::head())
            .map(|target| target.as_ref())
    }
}

#[derive(Debug, Error)]
pub enum InsertionError {
    #[error("inserting symbolic reference '{name} → {target}' would create a cycle")]
    Cyclic { name: RawName, target: RawTarget },

    #[error(transparent)]
    Protected(#[from] protect::Error),
}

// Mutability.
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

        self.0.insert(name, target);
        Ok(())
    }

    /// Try to insert a symbolic reference.
    /// Errors if `name` or `target` is protected (see [`protect`]) or would
    /// cause infinite recursion (e.g. `A → B` and `B → A`).
    pub fn try_insert(&mut self, name: RawName, target: RawTarget) -> Result<(), InsertionError> {
        self.try_insert_unprotected(Unprotected::new(name)?, Unprotected::new(target)?)
    }

    /// Consume `other` by iteratively inserting into self.
    pub fn extend(&mut self, other: SymbolicRefs) -> Result<(), InsertionError> {
        for (name, target) in other.0 {
            self.try_insert_unprotected(name, target)?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
#[error("symbolic reference '{name}' is cyclic")]
pub struct Cyclic {
    name: RawName,
}

impl TryFrom<BTreeMap<Name, Target>> for SymbolicRefs {
    type Error = Cyclic;

    fn try_from(map: BTreeMap<Name, Target>) -> Result<Self, Self::Error> {
        for (name, target) in map.iter() {
            if reachable(&map, target, name) {
                return Err(Cyclic {
                    name: name.to_owned().into_inner(),
                });
            }
        }

        Ok(Self(map))
    }
}

mod reachability {
    pub(super) trait Get<'a, 'b, K, V> {
        fn get(&'a self, key: &'b K) -> Option<&'a V>;
    }

    impl<'a, 'b, K: Ord, V> Get<'a, 'b, K, V> for std::collections::BTreeMap<K, V> {
        fn get(&'a self, key: &'b K) -> Option<&'a V> {
            std::collections::BTreeMap::get(self, key)
        }
    }

    impl<'a, 'b, K: Eq + std::hash::Hash, V> Get<'a, 'b, K, V> for std::collections::HashMap<K, V> {
        fn get(&'a self, key: &'b K) -> Option<&'a V> {
            std::collections::HashMap::get(self, key)
        }
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
    use crate::git::refname;

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

        assert_matches!(symbolic.try_insert(refname!("a"), refname!("b")), Ok(()));

        assert_matches!(symbolic.try_insert(refname!("b"), refname!("c")), Ok(()));

        assert_matches!(
            symbolic.try_insert(refname!("c"), refname!("a")),
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
    }

    /// Motivates why we cannot simply delegate to [`BTreeMap::extend`]
    /// for combining [`SymbolicRefs`].
    #[test]
    fn infinite_extend() {
        let mut a = SymbolicRefs::default();
        assert_matches!(a.try_insert(refname!("a"), refname!("b")), Ok(()));

        let mut b = SymbolicRefs::default();
        assert_matches!(b.try_insert(refname!("b"), refname!("a")), Ok(()));

        assert_matches!(a.extend(b), Err(InsertionError::Cyclic { .. }));
    }
}

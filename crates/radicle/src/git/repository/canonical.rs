//! Canonical Git reference access.
//!
//! [`Service`] provides operations to evaluate and update canonical references
//! within a Git repository. It acts as a facade over the underlying repository,
//! enforcing the rules defined in the identity document, ensuring that updates
//! only succeed if they meet the required quorum and convergence criteria.

#[cfg(test)]
mod test;

pub mod error;

use crate::git::Oid;
use crate::git::canonical::{Object, Rules};
use crate::git::fmt::Qualified;
use crate::git::repository::{Ancestry, object, reference};
use crate::prelude::Did;

// TODO: Rework documentation to mention [`Namespace`] rather than "service".

/// A service for managing and evaluating canonical references.
///
/// This acts as a domain-specific facade over a Git repository. It enforces
/// the rules defined in an identity document (represented by [`Rules`]),
/// ensuring that updates to shared references (like `refs/heads/main`) only
/// succeed if they meet the required delegate quorum and convergence criteria.
pub struct Namespace<'a, R> {
    repo: &'a R,
    rules: Rules,
}

impl<'a, R> Namespace<'a, R> {
    /// Construct a new canonical namespace using the provided rules.
    pub fn new(repo: &'a R, rules: Rules) -> Self {
        Self { repo, rules }
    }

    /// The rules governing this canonical namespace.
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// Returns `true` if the reference is governed by canonical rules.
    pub fn is_canonical(&self, name: &Qualified) -> bool {
        self.rules.matches(name).next().is_some()
    }
}

impl<'a, R> Namespace<'a, R>
where
    R: reference::Reader,
{
    /// Resolve a reference to its target [`Oid`].
    ///
    /// Returns `None` if the reference does not exist for this user.
    ///
    /// # Errors
    ///
    /// - [`Backend`]: An unexpected error from the underlying git library.
    ///
    /// [`Backend`]: reference::error::read::RefTarget::Backend
    pub fn ref_target(
        &self,
        name: &Qualified,
    ) -> Result<Option<Oid>, reference::error::read::RefTarget> {
        if self.is_canonical(name) {
            self.repo.ref_target(&name)
        } else {
            // TODO: Consider whether throwing an error here. Other option would
            // be always try and resolve, or return `None`.
            todo!()
        }
    }
}

impl<'a, R> Namespace<'a, R>
where
    R: reference::Writer + reference::Reader + object::Reader + Ancestry,
{
    /// Propose an update to a canonical reference.
    ///
    /// This is typically used during a `git push` operation. It evaluates whether
    /// the `target` object proposed by the `proposer` converges with the current
    /// state of other delegates (e.g. ensuring there are no diverging commits).
    ///
    /// If the convergence check passes and the delegate quorum is met, the
    /// canonical reference is updated in the underlying repository.
    ///
    /// # Errors
    ///
    /// Returns an [`error::Update`] if:
    /// - The target object cannot be found or is of an invalid kind.
    /// - The proposed update diverges from other delegates ([`QuorumError::Convergence`]).
    /// - The quorum threshold is not met ([`QuorumError::NoCandidates`]).
    /// - Writing to the underlying repository fails.
    ///
    /// [`QuorumError::Convergence`]: crate::git::canonical::error::QuorumError::Convergence
    /// [`QuorumError::NoCandidates`]: crate::git::canonical::error::QuorumError::NoCandidates
    pub fn propose(
        &self,
        name: &Qualified,
        target: Oid,
        proposer: Did,
        reflog: &str,
    ) -> Result<Option<Object>, error::Update> {
        let Some(canonical_eval) = self.rules.canonical(name.clone(), self.repo) else {
            return Ok(None);
        };

        let kind = self
            .repo
            .object_kind(target)?
            .ok_or(error::Update::ObjectNotFound(target))?;
        let obj =
            Object::from_kind(target, kind).ok_or(error::Update::InvalidObjectKind(target))?;

        let quorum = canonical_eval
            .find_objects()?
            .with_convergence(proposer, obj)
            .quorum()?
            .quorum;

        self.write_if_changed(name, quorum.object, reflog)
    }

    /// Re-evaluate the quorum of a canonical reference.
    ///
    /// This is typically used during a `radicle-fetch` operation. It tallies the
    /// current references of all delegates to determine the network's consensus.
    ///
    /// If a quorum is reached and the resulting target differs from the current
    /// canonical reference, the reference is updated in the underlying repository.
    ///
    /// # Errors
    ///
    /// Returns an [`error::Update`] if:
    /// - The delegates have diverged and no consensus can be reached.
    /// - The quorum threshold is not met.
    /// - Writing to the underlying repository fails.
    pub fn reevaluate(
        &self,
        name: &Qualified,
        reflog: &str,
    ) -> Result<Option<Object>, error::Update> {
        let Some(canonical_eval) = self.rules.canonical(name.clone(), self.repo) else {
            return Ok(None);
        };

        let quorum = canonical_eval.find_objects()?.quorum()?;

        self.write_if_changed(name, quorum.object, reflog)
    }

    /// Helper to only write to the repository if the OID actually changed.
    fn write_if_changed(
        &self,
        name: &Qualified,
        new_target: Object,
        reflog: &str,
    ) -> Result<Option<Object>, error::Update> {
        let current_target = self.repo.ref_target(name)?;

        if current_target != Some(new_target.id()) {
            self.repo.write_ref(
                name,
                reference::Target::Upsert {
                    target: new_target.id(),
                },
                reflog,
            )?;
            Ok(Some(new_target))
        } else {
            Ok(None)
        }
    }
}

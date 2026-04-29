use radicle::git;
use radicle::git::canonical;
use radicle::git::canonical::QuorumWithConvergence;
use radicle::git::canonical::effects;
use radicle::git::canonical::error::QuorumError;
use radicle::prelude::Did;

/// Validates a vote to update a canonical reference during push.
pub(crate) struct Canonical<'a, 'b, 'r, R> {
    canonical: canonical::CanonicalWithConvergence<'a, 'b, 'r, R>,
}

impl<'a, 'b, 'r, R> Canonical<'a, 'b, 'r, R>
where
    R: effects::Ancestry + effects::FindMergeBase + effects::FindObjects,
{
    pub(super) fn new(
        me: Did,
        object: canonical::Object,
        canonical: canonical::Canonical<'a, 'b, 'r, R, canonical::Initial>,
    ) -> Result<Self, canonical::error::FindObjectsError> {
        let canonical = canonical.find_objects()?;
        Ok(Self {
            canonical: canonical.with_convergence(me, object),
        })
    }

    /// Calculates the quorum of the [`git::canonical::Canonical`] provided.
    ///
    /// In some cases, it ensures that the head commit is attempting to converge
    /// with the set of commits of the other [`Did`]s.
    ///
    /// If a quorum is found, then it is also ensured that the new head commit
    /// is a descendant of the current canonical commit; otherwise, the commits
    /// are considered diverging.
    ///
    /// # Errors
    ///
    /// Ensures that the commits of the other [`Did`]s are in the working
    /// copy, and that checks that any two commits are related in the graph.
    ///
    /// Ensures that the new head and the canonical commit do not diverge.
    pub(super) fn quorum(
        self,
    ) -> Result<(git::fmt::Qualified<'a>, canonical::Object), QuorumError> {
        self.canonical
            .quorum()
            .map(|QuorumWithConvergence { quorum, .. }| (quorum.refname, quorum.object))
    }
}

pub(crate) mod io {
    use radicle::git::canonical::error::QuorumError;

    use crate::push::error;
    use crate::warn;

    /// Handle recoverable errors, printing relevant information to the
    /// terminal. Otherwise, convert the error into an unrecoverable error
    /// [`error::CanonicalUnrecoverable`].
    pub(crate) fn handle_error(e: QuorumError) -> Result<(), error::CanonicalUnrecoverable> {
        match e {
            QuorumError::Convergence(err) => Err(err.into()),
            QuorumError::MergeBase(err) => Err(err.into()),
            e @ QuorumError::DivergingCommits { .. } => {
                warn(e.to_string());
                warn("it is recommended to find a commit to agree upon");
                Ok(())
            }
            e @ QuorumError::DivergingTags { .. } => {
                warn(e.to_string());
                warn("it is recommended to find a tag to agree upon");
                Ok(())
            }
            e @ QuorumError::DifferentTypes { .. } => {
                warn(e.to_string());
                warn(
                    "it is recommended to find an object type (either commit or tag) to agree upon",
                );
                Ok(())
            }
            e @ QuorumError::NoCandidates { .. } => {
                warn(e.to_string());
                warn("it is recommended to find an object (either commit or tag) to agree upon");
                Ok(())
            }
        }
    }
}

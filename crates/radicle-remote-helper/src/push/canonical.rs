use radicle::git;
use radicle::git::canonical;
use radicle::git::canonical::effects;
use radicle::git::raw::Repository;
use radicle::prelude::Did;

use super::error;

pub(crate) struct Vote {
    object: canonical::Object,
}

impl Vote {
    pub(crate) fn id(&self) -> git::Oid {
        self.object.id()
    }
}

/// Validates a vote to update a canonical reference during push.
pub(crate) struct Canonical<'a, 'b, 'r, R> {
    vote: Vote,
    canonical: canonical::CanonicalWithConvergence<'a, 'b, 'r, R>,
}

impl<'a, 'b, 'r, R> Canonical<'a, 'b, 'r, R>
where
    R: effects::Ancestry + effects::FindMergeBase + effects::FindObjects,
{
    pub fn new(
        me: Did,
        object: canonical::Object,
        canonical: canonical::Canonical<'a, 'b, 'r, R, canonical::Initial>,
    ) -> Result<Self, canonical::error::FindObjectsError> {
        let canonical = canonical.find_objects()?;
        Ok(Self {
            vote: Vote { object },
            canonical: canonical.with_convergence(me, object),
        })
    }

    /// Calculates the quorum of the [`git::canonical::Canonical`] provided.
    ///
    /// In some cases, it ensures that the [`head`] is attempting to converge
    /// with the set of commits of the other [`Did`]s.
    ///
    /// If a quorum is found, then it is also ensured that the new [`head`] is a
    /// descendant of the current canonical commit, otherwise the commits are
    /// considered diverging.
    ///
    /// # Errors
    ///
    /// Ensures that the commits of the other [`Did`]s are in the working
    /// copy, and that checks that any two commits are related in the graph.
    ///
    /// Ensures that the new head and the canonical commit do not diverge.
    ///
    /// [`head`]: crate::push::canonical::Canonical::head
    pub fn quorum(
        self,
        working: &Repository,
    ) -> Result<(git::Qualified<'a>, canonical::Object), error::Canonical> {
        match self.canonical.quorum() {
            Ok(result) => {
                let canonical::QuorumWithConvergence { quorum, converges } = result;
                let canonical::Quorum { refname, object } = quorum;
                let quorum_head = object.id();
                // Canonical head is an ancestor of head.
                let is_ff = self.vote.id() == quorum_head
                    || working
                        .graph_descendant_of(*self.vote.id(), *quorum_head)
                        .map_err(|err| {
                            error::Canonical::graph_descendant(self.vote.id(), quorum_head, err)
                        })?;

                if !is_ff && !converges {
                    Err(error::Canonical::heads_diverge(self.vote.id(), quorum_head))
                } else {
                    Ok((refname, object))
                }
            }
            Err(err) => Err(err.into()),
        }
    }
}

pub(crate) mod io {
    use radicle::git;
    use radicle::git::canonical::error::QuorumError;

    use crate::push::error;
    use crate::{hint, warn};

    /// Handle recoverable errors, printing relevant information to the
    /// terminal. Otherwise, convert the error into an unrecoverable error
    /// [`error::CanonicalUnrecoverable`].
    pub fn handle_error(
        e: error::Canonical,
        canonical: &git::Qualified,
        hints: bool,
    ) -> Result<(), error::CanonicalUnrecoverable> {
        match e {
            error::Canonical::GraphDescendant(e) => Err(e.into()),
            error::Canonical::HeadsDiverge(e) => {
                if hints {
                    hint(
                        format!("you are attempting to push a commit that would cause \
                                                 your upstream to diverge from the canonical reference {canonical}"),
                    );
                    hint(
                        "to integrate the remote changes, run `git pull --rebase` \
                                                 and try again",
                    );
                }
                Err(e.into())
            }
            error::Canonical::Quorum(e) => {
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
                        warn("it is recommended to find an object type (either commit or tag) to agree upon");
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
    }
}

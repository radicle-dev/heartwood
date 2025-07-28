use radicle::git;
use radicle::git::raw::Repository;
use radicle::prelude::Did;

use super::error;

pub(crate) struct Vote {
    did: Did,
    oid: git::Oid,
    kind: git::canonical::CanonicalObject,
}

/// Validates a vote to update a canonical reference during push.
pub(crate) struct Canonical<'a, 'b> {
    vote: Vote,
    canonical: git::canonical::Canonical<'a, 'b>,
}

impl<'a, 'b> Canonical<'a, 'b> {
    pub fn new(
        me: Did,
        head: git::Oid,
        kind: git::canonical::CanonicalObject,
        canonical: git::canonical::Canonical<'a, 'b>,
    ) -> Self {
        Self {
            vote: Vote {
                did: me,
                oid: head,
                kind,
            },
            canonical,
        }
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
        mut self,
        working: &Repository,
    ) -> Result<(git::Qualified<'a>, git::raw::ObjectType, git::Oid), error::Canonical> {
        let converges = self
            .canonical
            .converges(working, (&self.vote.did, &self.vote.oid))?;
        if converges || self.canonical.has_no_tips() || self.canonical.is_only(&self.vote.did) {
            self.canonical
                .modify_vote(self.vote.did, self.vote.oid, self.vote.kind);
        }

        match self.canonical.quorum(working) {
            Ok((cref, quorum_type, quorum_head)) => {
                // Canonical head is an ancestor of head.
                let is_ff = self.vote.oid == quorum_head
                    || working
                        .graph_descendant_of(*self.vote.oid, *quorum_head)
                        .map_err(|err| {
                            error::Canonical::graph_descendant(self.vote.oid, quorum_head, err)
                        })?;

                if !is_ff && !converges {
                    Err(error::Canonical::heads_diverge(self.vote.oid, quorum_head))
                } else {
                    Ok((cref, quorum_type, quorum_head))
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
            error::Canonical::Converges(e) => Err(e.into()),
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
            error::Canonical::Quorum(e) => match e {
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
                    warn("it is recommended to find a commit to agree upon");
                    Ok(())
                }
                QuorumError::Git(err) => Err(error::CanonicalUnrecoverable::Git { source: err }),
            },
        }
    }
}

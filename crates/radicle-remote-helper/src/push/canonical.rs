use radicle::git;
use radicle::git::canonical::converges;
use radicle::git::raw::Repository;
use radicle::prelude::Did;

use super::error;

pub(crate) struct Vote {
    did: Did,
    commit: git::Oid,
}

/// Validates a vote to update a canonical reference during push.
pub(crate) struct Canonical {
    vote: Vote,
    canonical: git::canonical::Canonical,
}

impl Canonical {
    pub fn new(me: Did, head: git::Oid, canonical: git::canonical::Canonical) -> Self {
        Self {
            vote: Vote {
                did: me,
                commit: head,
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
    pub fn quorum(mut self, working: &Repository) -> Result<git::Oid, error::Canonical> {
        let heads = {
            let mut heads = self.canonical.tips();
            heads.try_fold(
                Vec::with_capacity(heads.size_hint().0),
                |mut heads, (did, head)| {
                    if *did != self.vote.did {
                        heads.push(Self::ensure_commit(*did, *head, working)?)
                    }
                    Ok::<_, error::Canonical>(heads)
                },
            )?
        };

        let converges = converges(heads.iter(), self.vote.commit, working)
            .map_err(|err| error::Canonical::converges(self.vote.commit, err))?;
        if converges {
            self.canonical.modify_vote(self.vote.did, self.vote.commit);
        }

        match self.canonical.quorum(working) {
            Ok(quorum_head) => {
                // Canonical head is an ancestor of head.
                let is_ff = self.vote.commit == quorum_head
                    || working
                        .graph_descendant_of(*self.vote.commit, *quorum_head)
                        .map_err(|err| {
                            error::Canonical::graph_descendant(self.vote.commit, quorum_head, err)
                        })?;

                if !is_ff && !converges {
                    Err(error::Canonical::heads_diverge(
                        self.vote.commit,
                        quorum_head,
                    ))
                } else {
                    Ok(quorum_head)
                }
            }
            Err(err) => Err(err.into()),
        }
    }

    fn ensure_commit(
        from: Did,
        commit: git::Oid,
        working: &Repository,
    ) -> Result<git::Oid, error::Canonical> {
        match working.find_commit(*commit).map(|_| commit) {
            Ok(oid) => Ok(oid),
            Err(err) if err.code() == git::raw::ErrorCode::NotFound => Err(
                error::Canonical::missing_commit(working.path().to_path_buf(), from, commit, err),
            ),
            Err(err) => Err(error::Canonical::invalid_commit(
                working.path().to_path_buf(),
                from,
                commit,
                err,
            )),
        }
    }
}

pub(crate) mod io {
    use radicle::git::{self, canonical};

    use crate::push::error;
    use crate::{hint, warn};

    /// Handle recoverable errors, printing relevant information to the
    /// terminal. Otherwise, convert the error into an unrecoverable error
    /// [`error::CanonicalUnrecoverable`].
    pub fn handle_error(
        e: error::Canonical,
        canonical: git::Qualified,
        hints: bool,
    ) -> Result<(), error::CanonicalUnrecoverable> {
        match e {
            error::Canonical::MissingCommit(e) => Err(e.into()),
            error::Canonical::InvalidCommit(e) => Err(e.into()),
            error::Canonical::GraphDescendant(e) => Err(e.into()),
            error::Canonical::Converges(e) => Err(e.into()),
            error::Canonical::HeadsDiverge(e) => {
                if hints {
                    hint(
                        "you are attempting to push a commit that would cause \
                                                 your upstream to diverge from the canonical head",
                    );
                    hint(
                        "to integrate the remote changes, run `git pull --rebase` \
                                                 and try again",
                    );
                }
                Err(e.into())
            }
            error::Canonical::Quorum(e) => match e {
                canonical::QuorumError::Diverging(e) => {
                    warn(format!(
                        "could not determine canonical tip for `{canonical}`"
                    ));
                    warn(e.to_string());
                    warn("it is recommended to find a commit to agree upon");
                    Ok(())
                }
                canonical::QuorumError::NoCandidates(e) => {
                    warn(format!(
                        "could not determine canonical tip for `{canonical}`"
                    ));
                    warn(e.to_string());
                    warn("it is recommended to find a commit to agree upon");
                    Ok(())
                }
                canonical::QuorumError::Git(err) => {
                    Err(error::CanonicalUnrecoverable::Git { source: err })
                }
            },
        }
    }
}

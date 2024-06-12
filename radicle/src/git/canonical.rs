use std::collections::BTreeMap;

use nonempty::NonEmpty;
use raw::Repository;
use thiserror::Error;

use crate::prelude::Did;
use crate::prelude::Project;
use crate::storage::ReadRepository;

use super::raw;
use super::{lit, Oid, Qualified};

/// A collection of [`Did`]s and their [`Oid`]s that is the tip for a given
/// reference for that [`Did`].
///
/// The general construction of `Canonical` is by using the
/// [`Canonical::reference`] constructor. For the default branch of a
/// [`Project`], use [`Canonical::default_branch`].
///
/// `Canonical` can then be used for performing calculations about the
/// canonicity of the reference, most importantly the [`Canonical::quorum`].
pub struct Canonical {
    tips: BTreeMap<Did, Oid>,
}

/// Error that can occur when calculation the [`Canonical::quorum`].
#[derive(Debug, Error)]
pub enum QuorumError {
    /// Could not determine a quorum [`Oid`].
    #[error("no quorum was  found")]
    NoQuorum,
    /// An error occurred from [`git2`].
    #[error(transparent)]
    Git(#[from] git2::Error),
}

impl Canonical {
    /// Construct the set of canonical tips of the `Project::default_branch` for
    /// the given `delegates`.
    pub fn default_branch<S>(
        repo: &S,
        project: &Project,
        delegates: &NonEmpty<Did>,
    ) -> Result<Self, raw::Error>
    where
        S: ReadRepository,
    {
        Self::reference(
            repo,
            delegates,
            &lit::refs_heads(project.default_branch()).into(),
        )
    }

    /// Construct the set of canonical tips given for the given `delegates` and
    /// the reference `name`.
    pub fn reference<S>(
        repo: &S,
        delegates: &NonEmpty<Did>,
        name: &Qualified,
    ) -> Result<Self, raw::Error>
    where
        S: ReadRepository,
    {
        let mut tips = BTreeMap::new();
        for delegate in delegates.iter() {
            match repo.reference_oid(delegate, name) {
                Ok(tip) => {
                    tips.insert(*delegate, tip);
                }
                Err(e) if super::ext::is_not_found_err(&e) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(Canonical { tips })
    }

    /// Return the set of [`Did`]s and their [`Oid`] tip.
    pub fn tips(&self) -> impl Iterator<Item = (&Did, &Oid)> {
        self.tips.iter()
    }
}

/// Check that a given `target` converges with any of the provided `tips`.
///
/// It converges if the `target` is either equal to, ahead of, or behind any of
/// the tips.
pub fn converges<'a>(
    tips: impl Iterator<Item = &'a Oid>,
    target: Oid,
    repo: &Repository,
) -> Result<bool, raw::Error> {
    for tip in tips {
        match repo.graph_ahead_behind(*target, **tip)? {
            (0, 0) => return Ok(true),
            (ahead, behind) if ahead > 0 && behind == 0 => return Ok(true),
            (ahead, behind) if behind > 0 && ahead == 0 => return Ok(true),
            (_, _) => {}
        }
    }
    Ok(false)
}

impl Canonical {
    /// In some cases, we allow the vote to be modified. For example, when the
    /// `did` is pushing a new commit, we may want to see if the new commit will
    /// reach a quorum.
    pub fn modify_vote(&mut self, did: Did, new: Oid) {
        self.tips.insert(did, new);
    }

    /// Computes the quorum or "canonical" tip based on the tips, of `Canonical`,
    /// and the threshold. This can be described as the latest commit that is
    /// included in at least `threshold` histories. In case there are multiple tips
    /// passing the threshold, and they are divergent, an error is returned.
    ///
    /// Also returns an error if `heads` is empty or `threshold` cannot be
    /// satisified with the number of heads given.
    pub fn quorum(&self, threshold: usize, repo: &raw::Repository) -> Result<Oid, QuorumError> {
        let mut candidates = BTreeMap::<_, usize>::new();

        // Build a list of candidate commits and count how many "votes" each of them has.
        // Commits get a point for each direct vote, as well as for being part of the ancestry
        // of a commit given to this function. Only commits given to the function are considered.
        for (i, head) in self.tips.values().enumerate() {
            // Add a direct vote for this head.
            *candidates.entry(*head).or_default() += 1;

            // Compare this head to all other heads ahead of it in the list.
            for other in self.tips.values().skip(i + 1) {
                // N.b. if heads are equal then skip it, otherwise it will end up as
                // a double vote.
                if *head == *other {
                    continue;
                }
                let base = Oid::from(repo.merge_base(**head, **other)?);

                if base == *other || base == *head {
                    *candidates.entry(base).or_default() += 1;
                }
            }
        }
        // Keep commits which pass the threshold.
        candidates.retain(|_, votes| *votes >= threshold);

        // Keep track of the longest identity branch.
        let (mut longest, _) = candidates.pop_first().ok_or(QuorumError::NoQuorum)?;

        // Now that all scores are calculated, figure out what is the longest branch
        // that passes the threshold. In case of divergence, return an error.
        for head in candidates.keys() {
            let base = repo.merge_base(**head, *longest)?;

            if base == *longest {
                // `head` is a successor of `longest`. Update `longest`.
                //
                //   o head
                //   |
                //   o longest (base)
                //   |
                //
                longest = *head;
            } else if base == **head || *head == longest {
                // `head` is an ancestor of `longest`, or equal to it. Do nothing.
                //
                //   o longest             o longest, head (base)
                //   |                     |
                //   o head (base)   OR    o
                //   |                     |
                //
            } else {
                // The merge base between `head` and `longest` (`base`)
                // is neither `head` nor `longest`. Therefore, the branches have
                // diverged.
                //
                //    longest   head
                //           \ /
                //            o (base)
                //            |
                //
                return Err(QuorumError::NoQuorum);
            }
        }
        Ok((*longest).into())
    }
}

use std::{cmp::Ordering, collections::BTreeMap};

use crate::git::Oid;

use super::voting::{CommitVoting, TagVoting};
use super::{MergeBase, Object};

/// [`TagQuorum`] encapsulates the process of voting on tag objects and
/// producing a quorum, if any.
#[derive(Debug)]
pub struct TagQuorum {
    threshold: usize,
    voting: TagVoting,
}

impl TagQuorum {
    /// Construct a new [`TagQuorum`] given a set of [`Object`]s and a
    /// `threshold`.
    pub fn new<'a, I>(objects: I, threshold: usize) -> Self
    where
        I: Iterator<Item = &'a Object>,
    {
        let voting = TagVoting::from_targets(objects.filter_map(|object| match object {
            Object::Commit { .. } => None,
            Object::Tag { id } => Some(*id),
        }));
        Self { threshold, voting }
    }

    /// Perform the quorum calculation and produce the [`Oid`] of the Git tag
    /// that passes the quorum, if any.
    pub fn find_quorum(self) -> Result<Oid, TagQuorumFailure> {
        let mut votes = self.voting.votes();
        votes.candidates_past_threshold(self.threshold);
        if votes.number_of_candidates() > 1 {
            Err(TagQuorumFailure::DivergingTags {
                candidates: votes.candidates().cloned().collect(),
            })
        } else {
            votes
                .max_candidate()
                .cloned()
                .ok_or(TagQuorumFailure::NoCandidates)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TagQuorumFailure {
    NoCandidates,
    DivergingTags { candidates: Vec<Oid> },
}

/// [`CommitQuorum`] encapsulates the process of voting on commit objects and
/// producing a quorum, if any.
///
/// Once constructed with [`CommitQuorum::new`],
/// [`CommitQuorum::next_candidate`] should be called. This produces a candidate
/// commit, and for each of the other commits, a merge base should be
/// calculated.
///
/// When a set of [`MergeBase`]s are found, it should be recorded using
/// [`CommitQuorum::found_merge_bases`].
///
/// Finally, [`CommitQuorum::find_quorum`] is used to calculate if there is a
/// quorum commit.
#[derive(Debug)]
pub struct CommitQuorum {
    threshold: usize,
    voting: CommitVoting,
    merge_bases: MergeBases,
}

/// The `MergeBaseKey` ensures that our [`MergeBases`] lookup table is
/// commutative when looking up a given merge base pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MergeBaseKey {
    a: Oid,
    b: Oid,
}

impl MergeBaseKey {
    /// Ensure the ordering is always stable
    fn new(a: Oid, b: Oid) -> Self {
        if a < b {
            Self { a, b }
        } else {
            Self { a: b, b: a }
        }
    }
}

impl PartialOrd for MergeBaseKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MergeBaseKey {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.a, self.b).cmp(&(other.a, other.b))
    }
}

/// A lookup table for merge bases, that is commutative in its keys. That is,
/// the following invariant should hold:
/// ```text, no_run
/// MergeBases::lookup(a, b) == MergeBases::lookup(b, a)
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MergeBases {
    lookup: BTreeMap<MergeBaseKey, Oid>,
}

impl MergeBases {
    /// Mark a [`MergeBase`] as found in the lookup table.
    fn found(
        &mut self,
        MergeBase {
            a: candidate,
            b: other,
            base,
        }: MergeBase,
    ) {
        self.lookup
            .insert(MergeBaseKey::new(candidate, other), base);
    }

    /// Lookup the base for two commits, `a` and `b` – note that this operation
    /// is commutative.
    fn lookup(&self, a: Oid, b: Oid) -> Option<&Oid> {
        self.lookup.get(&MergeBaseKey::new(a, b))
    }
}

impl CommitQuorum {
    /// Construct a new [`CommitQuorum`] given a set of [`Object`]s and a
    /// `threshold`.
    pub fn new<'a, I>(objects: I, threshold: usize) -> Self
    where
        I: Clone + Iterator<Item = &'a Object>,
    {
        let voting = CommitVoting::from_targets(objects.filter_map(|object| match object {
            Object::Commit { id } => Some(*id),
            Object::Tag { .. } => None,
        }));
        Self {
            threshold,
            voting,
            merge_bases: MergeBases::default(),
        }
    }

    /// Produces an iterator of the candidate commit paired with commits to
    /// compare against.
    ///
    /// A [`MergeBase`] should be calculated for each, and these should be
    /// recorded using [`CommitQuorum::found_merge_bases`].
    pub fn next_candidate(&mut self) -> Option<impl Iterator<Item = (Oid, Oid)>> {
        self.voting.next_candidate()
    }

    /// Record the [`MergeBase`]s for the [`CommitQuorum`].
    pub fn found_merge_bases(&mut self, bases: impl IntoIterator<Item = MergeBase>) {
        for base in bases {
            self.voting.found_merge_base(base);
            self.merge_bases.found(base);
        }
    }

    /// Perform the quorum calculation and produce the [`Oid`] of the Git commit
    /// that passes the quorum, if any.
    pub fn find_quorum(self) -> Result<Oid, CommitQuorumFailure> {
        let mut votes = self.voting.votes();
        votes.candidates_past_threshold(self.threshold);
        let mut longest = votes
            .pop_first_candidate()
            .ok_or(CommitQuorumFailure::NoCandidates)?;
        for candidate in votes.candidates() {
            let base = self.merge_bases.lookup(*candidate, longest).ok_or(
                CommitQuorumFailure::NoMergeBase {
                    a: *candidate,
                    b: longest,
                },
            )?;
            if *base == longest {
                // `head` is a successor of `longest`. Update `longest`.
                //
                //   o head
                //   |
                //   o longest (base)
                //   |
                //
                longest = *candidate;
            } else if base == candidate || *candidate == longest {
                // `head` is an ancestor of `longest`, or equal to it. Do nothing.
                //
                //   o longest             o longest, head (base)
                //   |                     |
                //   o head (base)   OR    o
                //   |                     |
                //
                continue;
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
                return Err(CommitQuorumFailure::DivergingCommits {
                    base: *base,
                    longest,
                    candidate: *candidate,
                });
            }
        }
        Ok(longest)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CommitQuorumFailure {
    NoCandidates,
    DivergingCommits {
        base: Oid,
        longest: Oid,
        candidate: Oid,
    },
    NoMergeBase {
        a: Oid,
        b: Oid,
    },
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use crate::git::{canonical::MergeBase, Oid};

    use super::MergeBases;

    fn commit(id: &str) -> Oid {
        id.parse().unwrap()
    }

    #[test]
    fn merge_base_commutative() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");

        let mut bases = MergeBases::default();
        bases.found(MergeBase {
            a: c2,
            b: c1,
            base: c0,
        });
        bases.found(MergeBase {
            a: c1,
            b: c2,
            base: c0,
        });
        assert_eq!(bases.lookup(c1, c2), Some(&c0));
        assert_eq!(bases.lookup(c2, c1), Some(&c0));
    }

    #[test]
    fn test_merge_bases() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        let b2 = commit("037a148170e3d41524b7c482a4798e5c2daeaa00");

        // B2 C2
        //   \|
        //   C1
        //   |
        //  C0
        let input = [
            MergeBase {
                a: b2,
                b: c2,
                base: c1,
            },
            MergeBase {
                a: c2,
                b: c1,
                base: c1,
            },
            MergeBase {
                a: b2,
                b: c1,
                base: c1,
            },
            MergeBase {
                a: c1,
                b: c0,
                base: c0,
            },
            MergeBase::trivial(b2),
            MergeBase::trivial(c2),
            MergeBase::trivial(c1),
            MergeBase::trivial(c0),
        ];
        let mut merge_bases = MergeBases::default();
        for i in input {
            merge_bases.found(i);
        }
        assert_eq!(merge_bases.lookup(b2, c2), Some(&c1));
    }
}

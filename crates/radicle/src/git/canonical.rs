use std::collections::BTreeMap;
use std::fmt;

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
#[derive(Debug)]
pub struct Canonical {
    tips: BTreeMap<Did, Oid>,
    threshold: usize,
}

/// Error that can occur when calculation the [`Canonical::quorum`].
#[derive(Debug, Error)]
pub enum QuorumError {
    /// Could not determine a quorum [`Oid`], due to diverging tips.
    #[error("could not determine canonical reference tip, {0}")]
    Diverging(Diverging),
    /// Could not determine a base candidate from the given set of delegates.
    #[error("could not determine canonical reference tip, {0}")]
    NoCandidates(NoCandidates),
    /// An error occurred from [`git2`].
    #[error(transparent)]
    Git(#[from] git2::Error),
}

/// No candidates were found for the [`Canonical::quorum`] calculation.
///
/// The [`fmt::Display`] is used in [`QuorumError`], to provide information on
/// the threshold and delegates in the calculation.
#[derive(Debug)]
pub struct NoCandidates {
    threshold: usize,
}

impl fmt::Display for NoCandidates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let NoCandidates { threshold } = self;
        write!(
            f,
            "no commit found with at least {threshold} vote(s) (threshold not met)"
        )
    }
}

/// Diverging commits were found during the [`Canonical::quorum`] calculation.
///
/// The [`fmt::Display`] is used in [`QuorumError`], to provide information on
/// the threshold, base commit, and the two diverging commits, in the
/// calculation.
#[derive(Debug)]
pub struct Diverging {
    threshold: usize,
    base: Oid,
    longest: Oid,
    head: Oid,
}

impl fmt::Display for Diverging {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Diverging {
            threshold,
            base,
            longest,
            head,
        } = self;
        write!(f, "found diverging commits {longest} and {head}, with base commit {base} and threshold {threshold}")
    }
}

impl Canonical {
    /// Construct the set of canonical tips of the `Project::default_branch` for
    /// the given `delegates`.
    pub fn default_branch<S>(
        repo: &S,
        project: &Project,
        delegates: &NonEmpty<Did>,
        threshold: usize,
    ) -> Result<Self, raw::Error>
    where
        S: ReadRepository,
    {
        Self::reference(
            repo,
            delegates,
            &lit::refs_heads(project.default_branch()).into(),
            threshold,
        )
    }

    /// Construct the set of canonical tips given for the given `delegates` and
    /// the reference `name`.
    pub fn reference<S>(
        repo: &S,
        delegates: &NonEmpty<Did>,
        name: &Qualified,
        threshold: usize,
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
                Err(e) if super::ext::is_not_found_err(&e) => {
                    log::warn!(
                        target: "radicle",
                        "Missing `refs/namespaces/{}/{name}` while calculating the canonical reference",
                        delegate.as_key()
                    );
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Canonical { tips, threshold })
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
    pub fn quorum(&self, repo: &raw::Repository) -> Result<Oid, QuorumError> {
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
        candidates.retain(|_, votes| *votes >= self.threshold);

        let (mut longest, _) =
            candidates
                .pop_first()
                .ok_or(QuorumError::NoCandidates(NoCandidates {
                    threshold: self.threshold,
                }))?;

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
                return Err(QuorumError::Diverging(Diverging {
                    threshold: self.threshold,
                    base: base.into(),
                    longest,
                    head: *head,
                }));
            }
        }
        Ok((*longest).into())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;
    use crate::assert_matches;
    use crate::git;
    use crate::node::device::Device;
    use crate::test::fixtures;

    /// Test helper to construct a Canonical and get the quorum
    fn quorum(
        heads: &[git::raw::Oid],
        threshold: usize,
        repo: &git::raw::Repository,
    ) -> Result<Oid, QuorumError> {
        let tips = heads
            .iter()
            .enumerate()
            .map(|(i, head)| {
                let signer = Device::mock_from_seed([(i + 1) as u8; 32]);
                let did = Did::from(signer.public_key());
                (did, (*head).into())
            })
            .collect();
        Canonical { tips, threshold }.quorum(repo)
    }

    #[test]
    fn test_quorum_properties() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, c0) = fixtures::repository(tmp.path());
        let c0: git::Oid = c0.into();
        let a1 = fixtures::commit("A1", &[*c0], &repo);
        let a2 = fixtures::commit("A2", &[*a1], &repo);
        let d1 = fixtures::commit("D1", &[*c0], &repo);
        let c1 = fixtures::commit("C1", &[*c0], &repo);
        let c2 = fixtures::commit("C2", &[*c1], &repo);
        let b2 = fixtures::commit("B2", &[*c1], &repo);
        let a1 = fixtures::commit("A1", &[*c0], &repo);
        let m1 = fixtures::commit("M1", &[*c2, *b2], &repo);
        let m2 = fixtures::commit("M2", &[*a1, *b2], &repo);
        let mut rng = fastrand::Rng::new();
        let choices = [*c0, *c1, *c2, *b2, *a1, *a2, *d1, *m1, *m2];

        for _ in 0..100 {
            let count = rng.usize(1..=choices.len());
            let threshold = rng.usize(1..=count);
            let mut heads = Vec::new();

            for _ in 0..count {
                let ix = rng.usize(0..choices.len());
                heads.push(choices[ix]);
            }
            rng.shuffle(&mut heads);

            if let Ok(canonical) = quorum(&heads, threshold, &repo) {
                assert!(heads.contains(&canonical));
            }
        }
    }

    #[test]
    fn test_quorum() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, c0) = fixtures::repository(tmp.path());
        let c0: git::Oid = c0.into();
        let c1 = fixtures::commit("C1", &[*c0], &repo);
        let c2 = fixtures::commit("C2", &[*c1], &repo);
        let c3 = fixtures::commit("C3", &[*c1], &repo);
        let b2 = fixtures::commit("B2", &[*c1], &repo);
        let a1 = fixtures::commit("A1", &[*c0], &repo);
        let m1 = fixtures::commit("M1", &[*c2, *b2], &repo);
        let m2 = fixtures::commit("M2", &[*a1, *b2], &repo);

        eprintln!("C0: {c0}");
        eprintln!("C1: {c1}");
        eprintln!("C2: {c2}");
        eprintln!("C3: {c3}");
        eprintln!("B2: {b2}");
        eprintln!("A1: {a1}");
        eprintln!("M1: {m1}");
        eprintln!("M2: {m2}");

        assert_eq!(quorum(&[*c0], 1, &repo).unwrap(), c0);
        assert_eq!(quorum(&[*c1], 1, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*c2], 1, &repo).unwrap(), c2);
        assert_eq!(quorum(&[*c0], 0, &repo).unwrap(), c0);
        assert_matches!(quorum(&[], 0, &repo), Err(QuorumError::NoCandidates(_)));
        assert_matches!(quorum(&[*c0], 2, &repo), Err(QuorumError::NoCandidates(_)));

        //  C1
        //  |
        // C0
        assert_eq!(quorum(&[*c1], 1, &repo).unwrap(), c1);

        //   C2
        //   |
        //  C1
        //  |
        // C0
        assert_eq!(quorum(&[*c1, *c2], 1, &repo).unwrap(), c2);
        assert_eq!(quorum(&[*c1, *c2], 2, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*c0, *c1, *c2], 3, &repo).unwrap(), c0);
        assert_eq!(quorum(&[*c1, *c1, *c2], 2, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*c1, *c1, *c2], 1, &repo).unwrap(), c2);
        assert_eq!(quorum(&[*c2, *c2, *c1], 1, &repo).unwrap(), c2);

        // B2 C2
        //   \|
        //   C1
        //   |
        //  C0
        assert_matches!(
            quorum(&[*c1, *c2, *b2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*c2, *b2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*b2, *c2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*c2, *b2], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*b2, *c2], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_eq!(quorum(&[*c1, *c2, *b2], 2, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*c1, *c2, *b2], 3, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*b2, *b2, *c2], 2, &repo).unwrap(), b2);
        assert_eq!(quorum(&[*b2, *c2, *c2], 2, &repo).unwrap(), c2);
        assert_matches!(
            quorum(&[*b2, *b2, *c2, *c2], 2, &repo),
            Err(QuorumError::Diverging(_))
        );

        // B2 C2 C3
        //  \ | /
        //    C1
        //    |
        //    C0
        assert_eq!(quorum(&[*b2, *c2, *c2], 2, &repo).unwrap(), c2);
        assert_matches!(
            quorum(&[*b2, *c2, *c2], 3, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*b2, *c2, *b2, *c2], 3, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*c3, *b2, *c2, *b2, *c2, *c3], 3, &repo),
            Err(QuorumError::NoCandidates(_))
        );

        //  B2 C2
        //    \|
        // A1 C1
        //   \|
        //   C0
        assert_matches!(
            quorum(&[*c2, *b2, *a1], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*c2, *b2, *a1], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*c2, *b2, *a1], 3, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*c1, *c2, *b2, *a1], 4, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_eq!(quorum(&[*c0, *c1, *c2, *b2, *a1], 2, &repo).unwrap(), c1,);
        assert_eq!(quorum(&[*c0, *c1, *c2, *b2, *a1], 3, &repo).unwrap(), c1,);
        assert_eq!(quorum(&[*c0, *c2, *b2, *a1], 3, &repo).unwrap(), c0);
        assert_eq!(quorum(&[*c0, *c1, *c2, *b2, *a1], 4, &repo).unwrap(), c0,);
        assert_matches!(
            quorum(&[*a1, *a1, *c2, *c2, *c1], 2, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*a1, *a1, *c2, *c2, *c1], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*a1, *a1, *c2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*b2, *b2, *c2, *c2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*b2, *b2, *c2, *c2, *a1], 1, &repo),
            Err(QuorumError::Diverging(_))
        );

        //    M2  M1
        //    /\  /\
        //    \ B2 C2
        //     \  \|
        //     A1 C1
        //       \|
        //       C0
        assert_eq!(quorum(&[*m1], 1, &repo).unwrap(), m1);
        assert_matches!(
            quorum(&[*m1, *m2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m2, *m1], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m1, *m2], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*m1, *m2, *c2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m1, *a1], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m1, *a1], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_eq!(quorum(&[*m1, *m2, *b2, *c1], 4, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*m1, *m1, *b2], 2, &repo).unwrap(), m1);
        assert_eq!(quorum(&[*m1, *m1, *c2], 2, &repo).unwrap(), m1);
        assert_eq!(quorum(&[*m2, *m2, *b2], 2, &repo).unwrap(), m2);
        assert_eq!(quorum(&[*m2, *m2, *a1], 2, &repo).unwrap(), m2);
        assert_eq!(quorum(&[*m1, *m1, *b2, *b2], 2, &repo).unwrap(), m1);
        assert_eq!(quorum(&[*m1, *m1, *c2, *c2], 2, &repo).unwrap(), m1);
        assert_eq!(quorum(&[*m1, *b2, *c1, *c0], 3, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*m1, *b2, *c1, *c0], 4, &repo).unwrap(), c0);
    }

    #[test]
    fn test_quorum_merges() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, c0) = fixtures::repository(tmp.path());
        let c0: git::Oid = c0.into();
        let c1 = fixtures::commit("C1", &[*c0], &repo);
        let c2 = fixtures::commit("C2", &[*c0], &repo);
        let c3 = fixtures::commit("C3", &[*c0], &repo);

        let m1 = fixtures::commit("M1", &[*c1, *c2], &repo);
        let m2 = fixtures::commit("M2", &[*c2, *c3], &repo);

        eprintln!("C0: {c0}");
        eprintln!("C1: {c1}");
        eprintln!("C2: {c2}");
        eprintln!("C3: {c3}");
        eprintln!("M1: {m1}");
        eprintln!("M2: {m2}");

        //    M2  M1
        //    /\  /\
        //   C1 C2 C3
        //     \| /
        //      C0
        assert_matches!(
            quorum(&[*m1, *m2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m1, *m2], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );

        let m3 = fixtures::commit("M3", &[*c2, *c1], &repo);

        //   M3/M2 M1
        //    /\  /\
        //   C1 C2 C3
        //     \| /
        //      C0
        assert_matches!(
            quorum(&[*m1, *m3], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m1, *m3], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*m3, *m1], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m3, *m1], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
        assert_matches!(
            quorum(&[*m3, *m2], 1, &repo),
            Err(QuorumError::Diverging(_))
        );
        assert_matches!(
            quorum(&[*m3, *m2], 2, &repo),
            Err(QuorumError::NoCandidates(_))
        );
    }
}

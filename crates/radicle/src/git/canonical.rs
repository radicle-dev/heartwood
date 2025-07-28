pub mod rules;
pub use rules::{MatchedRule, RawRule, Rules, ValidRule};

use std::collections::BTreeMap;
use std::path::PathBuf;

use raw::ObjectType;
use raw::Repository;
use thiserror::Error;

use crate::prelude::Did;

use super::raw;
use super::{Oid, Qualified};

/// A collection of [`Did`]s and their [`Oid`]s that is the tip for a given
/// reference for that [`Did`].
///
/// The general construction of `Canonical` is by using the [`Canonical::new`]
/// constructor.
///
/// `Canonical` can then be used for performing calculations about the
/// canonicity of the reference, most importantly the [`Canonical::quorum`].
///
/// References to the refname and the matched rule are kept, as they
/// are very handy for generating error messages.
#[derive(Debug)]
pub struct Canonical<'a, 'b> {
    refname: Qualified<'a>,
    rule: &'b ValidRule,
    tips: BTreeMap<Did, (Oid, git2::ObjectType)>,
}

/// Error that can occur when calculation the [`Canonical::quorum`].
#[derive(Debug, Error)]
pub enum QuorumError {
    /// Could not determine a quorum [`Oid`], due to diverging tips.
    #[error("could not determine target commit for canonical reference '{refname}', found diverging commits {longest} and {head}, with base commit {base} and threshold {threshold}")]
    DivergingCommits {
        refname: String,
        threshold: usize,
        base: Oid,
        longest: Oid,
        head: Oid,
    },
    #[error("could not determine target tag for canonical reference '{refname}', found multiple candidates with threshold {threshold}")]
    DivergingTags {
        refname: String,
        threshold: usize,
        candidates: Vec<Oid>,
    },
    #[error("could not determine target for canonical reference '{refname}', found objects of different types")]
    DifferentTypes { refname: String },
    /// Could not determine a base candidate from the given set of delegates.
    #[error("could not determine target for canonical reference '{refname}', no object with at least {threshold} vote(s) found (threshold not met)")]
    NoCandidates { refname: String, threshold: usize },
    /// An error occurred from [`git2`].
    #[error(transparent)]
    Git(#[from] git2::Error),
}

#[derive(Debug, Error)]
#[error("failed to check if {head} is an ancestor of {canonical} due to: {source}")]
pub struct GraphDescendant {
    head: Oid,
    canonical: Oid,
    source: raw::Error,
}

#[derive(Debug, Error)]
#[error("the commit {commit} for {did} is missing from the repository {repo:?}")]
pub struct MissingObject {
    repo: PathBuf,
    did: Did,
    commit: Oid,
    source: raw::Error,
}

#[derive(Debug, Error)]
#[error("could not determine whether the commit {commit} for {did} is part of the repository {repo:?} due to: {source}")]
pub struct InvalidObject {
    repo: PathBuf,
    did: Did,
    commit: Oid,
    source: raw::Error,
}

#[derive(Debug, Error)]
#[error("the object {oid} for {did} in the repository {repo:?} is of unexpected type {kind:?}")]
pub struct InvalidObjectType {
    repo: PathBuf,
    did: Did,
    oid: Oid,
    kind: Option<git2::ObjectType>,
}

#[derive(Debug, Error)]
pub enum ConvergesError {
    #[error(transparent)]
    GraphDescendant(#[from] GraphDescendant),
    #[error(transparent)]
    MissingObject(#[from] MissingObject),
    #[error(transparent)]
    InvalidObject(#[from] InvalidObject),
    #[error(transparent)]
    InvalidObjectType(#[from] InvalidObjectType),
}

impl ConvergesError {
    pub fn graph_descendant(head: Oid, canonical: Oid, source: raw::Error) -> Self {
        Self::GraphDescendant(GraphDescendant {
            head,
            canonical,
            source,
        })
    }

    pub fn missing_object(repo: PathBuf, did: Did, commit: Oid, err: raw::Error) -> Self {
        Self::MissingObject(MissingObject {
            repo,
            did,
            commit,
            source: err,
        })
    }

    pub fn invalid_object(repo: PathBuf, did: Did, commit: Oid, err: raw::Error) -> Self {
        Self::InvalidObject(InvalidObject {
            repo,
            did,
            commit,
            source: err,
        })
    }

    pub fn invalid_object_kind(
        repo: PathBuf,
        did: Did,
        oid: Oid,
        kind: Option<git2::ObjectType>,
    ) -> Self {
        Self::InvalidObjectType(InvalidObjectType {
            repo,
            did,
            oid,
            kind,
        })
    }
}

impl<'a, 'b> Canonical<'a, 'b> {
    /// Construct the set of canonical tips given for the given `rule` and
    /// the reference `refname`.
    pub fn new(
        repo: &Repository,
        refname: Qualified<'a>,
        rule: &'b ValidRule,
    ) -> Result<Self, raw::Error> {
        let mut tips = BTreeMap::new();
        for delegate in rule.allowed().iter() {
            let name = &refname.with_namespace(delegate.as_key().into());

            let reference = match repo.find_reference(&name) {
                Ok(reference) => reference,
                Err(e) if super::ext::is_not_found_err(&e) => {
                    log::warn!(
                        target: "radicle",
                        "Missing `refs/namespaces/{}/{refname}` while calculating the canonical reference",
                        delegate.as_key()
                    );
                    continue;
                }
                Err(e) => return Err(e),
            };

            let Some(oid) = reference.target() else {
                continue;
            };

            let Some(kind) = repo.find_object(oid, None)?.kind() else {
                continue;
            };

            tips.insert(*delegate, (oid.into(), kind));
        }
        Ok(Canonical {
            refname,
            tips,
            rule,
        })
    }

    /// Returns `true` if there were no tips found for any of the DIDs for
    /// the given reference.
    ///
    /// N.b. this may be the case when a new reference is being created.
    pub fn has_no_tips(&self) -> bool {
        self.tips.is_empty()
    }

    pub fn refname(&self) -> &Qualified {
        &self.refname
    }

    /// In some cases, we allow the vote to be modified. For example, when the
    /// `did` is pushing a new commit, we may want to see if the new commit will
    /// reach a quorum.
    pub fn modify_vote(&mut self, did: Did, new: (Oid, git2::ObjectType)) {
        self.tips.insert(did, new);
    }

    /// Check that the provided `did` is part of the set of allowed
    /// DIDs of the matching rule.
    pub fn is_allowed(&self, did: &Did) -> bool {
        self.rule.allowed().contains(did)
    }

    /// Check that the provided `did` is the only DID in the set of allowed
    /// DIDs of the matching rule.
    pub fn is_only(&self, did: &Did) -> bool {
        self.rule.allowed().is_only(did)
    }

    /// Checks that setting the given candidate tip would converge with at least
    /// one other known tip.
    ///
    /// It converges if the candidate Oid is either equal to, ahead of, or behind any of
    /// the tips.
    pub fn converges(
        &self,
        repo: &Repository,
        (candidate, commit): (&Did, &Oid),
    ) -> Result<bool, ConvergesError> {
        let mut common_kind = ObjectType::Any;
        let heads = {
            let heads = self
                .tips
                .iter()
                .filter_map(|(did, tip)| (did != candidate).then_some((did, tip)));

            let mut result = Vec::with_capacity(heads.size_hint().0);

            for (did, (oid, kind)) in heads {
                if common_kind == ObjectType::Any {
                    common_kind = *kind;
                } else if common_kind != *kind {
                    return Err(ConvergesError::invalid_object_kind(
                        repo.path().to_path_buf(),
                        *did,
                        *oid,
                        Some(*kind),
                    ));
                }
                result.push(Self::ensure_commit_or_tag(*did, *oid, repo)?);
            }

            result
        };

        if common_kind == ObjectType::Commit {
            for (head, _) in heads {
                let (ahead, behind) = repo
                    .graph_ahead_behind(**commit, *head)
                    .map_err(|err| ConvergesError::graph_descendant(*commit, head, err))?;
                if ahead * behind == 0 {
                    return Ok(true);
                }
            }
        } else {
            return Ok(true);
        }
        Ok(false)
    }

    fn quorum_tag(&self) -> Result<Oid, QuorumError> {
        let mut candidates = BTreeMap::<Oid, u8>::new();

        for (head, kind) in self.tips.values() {
            if *kind != raw::ObjectType::Tag {
                continue;
            }
            {
                let votes = candidates.entry(*head).or_default();
                *votes = votes.saturating_add(1);
            }
        }

        // Keep tags which pass the threshold.
        candidates.retain(|_, votes| *votes as usize >= self.threshold());

        if candidates.len() > 1 {
            return Err(QuorumError::DivergingTags {
                refname: self.refname.to_string(),
                threshold: self.threshold(),
                candidates: candidates.keys().cloned().collect(),
            });
        }

        let (longest, _) = candidates.pop_first().ok_or(QuorumError::NoCandidates {
            refname: self.refname.to_string(),
            threshold: self.threshold(),
        })?;

        Ok((*longest).into())
    }

    /// Computes the quorum or "canonical" tip based on the tips, of `Canonical`,
    /// and the threshold. This can be described as the latest commit that is
    /// included in at least `threshold` histories. In case there are multiple tips
    /// passing the threshold, and they are divergent, an error is returned.
    ///
    /// Also returns an error if `heads` is empty or `threshold` cannot be
    /// satisified with the number of heads given.
    fn quorum_commit(&self, repo: &raw::Repository) -> Result<Oid, QuorumError> {
        let mut candidates = BTreeMap::<Oid, u8>::new();

        // Build a list of candidate commits and count how many "votes" each of them has.
        // Commits get a point for each direct vote, as well as for being part of the ancestry
        // of a commit given to this function. Only commits given to the function are considered.
        for (i, (head, kind)) in self.tips.values().enumerate() {
            if *kind != raw::ObjectType::Commit {
                continue;
            }
            {
                let votes = candidates.entry(*head).or_default();
                *votes = votes.saturating_add(1);
            }
            // Compare this head to all other heads ahead of it in the list.
            for (other, kind) in self.tips.values().skip(i + 1) {
                if *kind != raw::ObjectType::Commit {
                    continue;
                }
                // N.b. if heads are equal then skip it, otherwise it will end up as
                // a double vote.
                if head == other {
                    continue;
                }

                let base = Oid::from(repo.merge_base(**head, **other)?);

                if base == *other || base == *head {
                    {
                        let votes = candidates.entry(base).or_default();
                        *votes = votes.saturating_add(1);
                    }
                }
            }
        }

        // Keep commits which pass the threshold.
        candidates.retain(|_, votes| *votes as usize >= self.threshold());

        let (mut longest, _) = candidates.pop_first().ok_or(QuorumError::NoCandidates {
            refname: self.refname.to_string(),
            threshold: self.threshold(),
        })?;

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
                return Err(QuorumError::DivergingCommits {
                    refname: self.refname.to_string(),
                    threshold: self.threshold(),
                    base: base.into(),
                    longest,
                    head: *head,
                });
            }
        }

        Ok((*longest).into())
    }

    /// Computes the quorum or "canonical" tip based on the tips, of `Canonical`,
    /// and the threshold. This can be described as the latest commit that is
    /// included in at least `threshold` histories. In case there are multiple tips
    /// passing the threshold, and they are divergent, an error is returned.
    ///
    /// Also returns an error if `heads` is empty or `threshold` cannot be
    /// satisified with the number of heads given.
    pub fn quorum(
        self,
        repo: &raw::Repository,
    ) -> Result<(Qualified<'a>, ObjectType, Oid), QuorumError> {
        let (oid, kind) = match (self.quorum_commit(repo), self.quorum_tag()) {
            (Ok(commit), Err(_)) => Ok((commit, ObjectType::Commit)),
            (Err(_), Ok(tag)) => Ok((tag, ObjectType::Tag)),
            (Ok(_), Ok(_)) => Err(QuorumError::DifferentTypes {
                refname: self.refname.clone().to_string(),
            }),
            (Err(commit), Err(QuorumError::NoCandidates { .. })) => Err(commit),
            (Err(QuorumError::NoCandidates { .. }), Err(tag)) => Err(tag),
            (Err(err), _) => Err(err),
        }?;

        Ok((self.refname, kind, oid))
    }

    fn threshold(&self) -> usize {
        (*self.rule.threshold()).into()
    }

    fn ensure_commit_or_tag(
        from: Did,
        commit_or_tag: Oid,
        working: &Repository,
    ) -> Result<(Oid, ObjectType), ConvergesError> {
        match working.find_object(*commit_or_tag, None) {
            Ok(object) => match object.kind() {
                Some(kind @ ObjectType::Commit) | Some(kind @ ObjectType::Tag) => {
                    Ok((object.id().into(), kind))
                }
                kind => Err(ConvergesError::invalid_object_kind(
                    working.path().to_path_buf(),
                    from,
                    commit_or_tag,
                    kind,
                )),
            },
            Err(err) if err.code() == raw::ErrorCode::NotFound => {
                Err(ConvergesError::missing_object(
                    working.path().to_path_buf(),
                    from,
                    commit_or_tag,
                    err,
                ))
            }
            Err(err) => Err(ConvergesError::invalid_object(
                working.path().to_path_buf(),
                from,
                commit_or_tag,
                err,
            )),
        }
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
        let tips: BTreeMap<Did, (Oid, git2::ObjectType)> = heads
            .iter()
            .enumerate()
            .map(|(i, head)| {
                let signer = Device::mock_from_seed([(i + 1) as u8; 32]);
                let did = Did::from(signer.public_key());
                let kind = repo.find_object(*head, None).unwrap().kind().unwrap();
                (did, ((*head).into(), kind))
            })
            .collect();

        let refname =
            git::refs::branch(git_ext::ref_format::RefStr::try_from_str("master").unwrap());

        let rule: RawRule = crate::git::canonical::rules::Rule::new(
            crate::git::canonical::rules::Allowed::Delegates,
            threshold,
        );
        let delegates = crate::identity::doc::Delegates::new(tips.keys().cloned()).unwrap();
        let rule = rule.validate(&mut || delegates.clone()).unwrap();

        Canonical {
            refname,
            tips,
            rule: &rule,
        }
        .quorum(repo)
        .map(|(_, _, oid)| oid)
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
    fn test_quorum_groups() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, c0) = fixtures::repository(tmp.path());
        let c0: git::Oid = c0.into();
        let c1 = fixtures::commit("C1", &[*c0], &repo);
        let c2 = fixtures::commit("C2", &[*c0], &repo);

        eprintln!("C0: {c0}");
        eprintln!("C1: {c1}");
        eprintln!("C2: {c2}");

        assert_matches!(
            quorum(&[*c1, *c2, *c1, *c2], 2, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );

        assert_matches!(
            quorum(&[*c1, *c2], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
    }

    #[test]
    fn test_quorum_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, c0) = fixtures::repository(tmp.path());
        let c0: git::Oid = c0.into();
        let c1 = fixtures::commit("C1", &[*c0], &repo);
        let t1 = fixtures::tag("v1", "T1", *c1, &repo);
        let t2 = fixtures::tag("v2", "T2", *c1, &repo);

        eprintln!("C0: {c0}");
        eprintln!("C1: {c1}");
        eprintln!("T1: {t1}");
        eprintln!("T2: {t2}");

        assert_eq!(quorum(&[*t1], 1, &repo).unwrap(), t1);
        assert_eq!(quorum(&[*t1, *t1], 2, &repo).unwrap(), t1);

        assert_matches!(
            quorum(&[*t1, *t2], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );

        assert_matches!(
            quorum(&[*t1, *c1], 1, &repo),
            Err(QuorumError::DifferentTypes { .. })
        );

        assert_matches!(
            quorum(&[*t1, *t2], 1, &repo),
            Err(QuorumError::DivergingTags { .. })
        );
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
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*c2, *b2], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*b2, *c2], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*c2, *b2], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*b2, *c2], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_eq!(quorum(&[*c1, *c2, *b2], 2, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*c1, *c2, *b2], 3, &repo).unwrap(), c1);
        assert_eq!(quorum(&[*b2, *b2, *c2], 2, &repo).unwrap(), b2);
        assert_eq!(quorum(&[*b2, *c2, *c2], 2, &repo).unwrap(), c2);
        assert_matches!(
            quorum(&[*b2, *b2, *c2, *c2], 2, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );

        // B2 C2 C3
        //  \ | /
        //    C1
        //    |
        //    C0
        assert_eq!(quorum(&[*b2, *c2, *c2], 2, &repo).unwrap(), c2);
        assert_matches!(
            quorum(&[*b2, *c2, *c2], 3, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*b2, *c2, *b2, *c2], 3, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*c3, *b2, *c2, *b2, *c2, *c3], 3, &repo),
            Err(QuorumError::NoCandidates { .. })
        );

        //  B2 C2
        //    \|
        // A1 C1
        //   \|
        //   C0
        assert_matches!(
            quorum(&[*c2, *b2, *a1], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*c2, *b2, *a1], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*c2, *b2, *a1], 3, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*c1, *c2, *b2, *a1], 4, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_eq!(quorum(&[*c0, *c1, *c2, *b2, *a1], 2, &repo).unwrap(), c1,);
        assert_eq!(quorum(&[*c0, *c1, *c2, *b2, *a1], 3, &repo).unwrap(), c1,);
        assert_eq!(quorum(&[*c0, *c2, *b2, *a1], 3, &repo).unwrap(), c0);
        assert_eq!(quorum(&[*c0, *c1, *c2, *b2, *a1], 4, &repo).unwrap(), c0,);
        assert_matches!(
            quorum(&[*a1, *a1, *c2, *c2, *c1], 2, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*a1, *a1, *c2, *c2, *c1], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*a1, *a1, *c2], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*b2, *b2, *c2, *c2], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*b2, *b2, *c2, *c2, *a1], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );

        //    M2  M1
        //    /\  /\
        //    \ B2 C2
        //     \  \|
        //     A1 C1
        //       \|
        //       C0
        // assert_eq!(quorum(&[*m1], 1, &repo).unwrap(), m1);
        // assert_matches!(
        //     quorum(&[*m1, *m2], 1, &repo),
        //     Err(QuorumError::DivergingCommits { .. })
        // );
        // assert_matches!(
        //     quorum(&[*m2, *m1], 1, &repo),
        //     Err(QuorumError::DivergingCommits { .. })
        // );
        // assert_matches!(
        //     quorum(&[*m1, *m2], 2, &repo),
        //     Err(QuorumError::NoCandidates { .. })
        // );
        // assert_matches!(
        //     quorum(&[*m1, *m2, *c2], 1, &repo),
        //     Err(QuorumError::DivergingCommits { .. })
        // );
        // assert_matches!(
        //     quorum(&[*m1, *a1], 1, &repo),
        //     Err(QuorumError::DivergingCommits { .. })
        // );
        // assert_matches!(
        //     quorum(&[*m1, *a1], 2, &repo),
        //     Err(QuorumError::NoCandidates { .. })
        // );
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
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*m1, *m2], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );

        let m3 = fixtures::commit("M3", &[*c2, *c1], &repo);

        //   M3/M2 M1
        //    /\  /\
        //   C1 C2 C3
        //     \| /
        //      C0
        assert_matches!(
            quorum(&[*m1, *m3], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*m1, *m3], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*m3, *m1], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*m3, *m1], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
        assert_matches!(
            quorum(&[*m3, *m2], 1, &repo),
            Err(QuorumError::DivergingCommits { .. })
        );
        assert_matches!(
            quorum(&[*m3, *m2], 2, &repo),
            Err(QuorumError::NoCandidates { .. })
        );
    }
}

pub mod error;
use error::*;

pub mod rules;

use nonempty::NonEmpty;
pub use rules::{MatchedRule, RawRule, Rules, ValidRule};

use std::collections::BTreeMap;
use std::fmt;

use raw::ObjectType;
use raw::Repository;

use crate::prelude::Did;
use crate::storage::git;

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
    tips: BTreeMap<Did, (Oid, CanonicalObject)>,
}

/// Support Git object types for canonical computation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalObject {
    /// The Git object corresponds to a commit.
    Commit,
    /// The Git object corresponds to a tag.
    Tag,
}

impl fmt::Display for CanonicalObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonicalObject::Commit => f.write_str("commit"),
            CanonicalObject::Tag => f.write_str("tag"),
        }
    }
}

impl CanonicalObject {
    /// Construct the [`CanonicalObject`] from a [`git2::ObjectType`].
    pub fn new(kind: git::raw::ObjectType) -> Option<Self> {
        match kind {
            ObjectType::Commit => Some(Self::Commit),
            ObjectType::Tag => Some(Self::Tag),
            _ => None,
        }
    }

    /// Returns `true` if the object is a commit.
    fn is_commit(&self) -> bool {
        match self {
            CanonicalObject::Commit => true,
            CanonicalObject::Tag => false,
        }
    }

    /// Returns `true` if the object is a tag.
    fn is_tag(&self) -> bool {
        match self {
            CanonicalObject::Commit => false,
            CanonicalObject::Tag => true,
        }
    }
}

impl<'a, 'b> Canonical<'a, 'b> {
    /// Construct the set of canonical tips given for the given `rule` and
    /// the reference `refname`.
    pub fn new(
        repo: &Repository,
        refname: Qualified<'a>,
        rule: &'b ValidRule,
    ) -> Result<Self, CanonicalError> {
        let mut tips = BTreeMap::new();
        for delegate in rule.allowed().iter() {
            let name = &refname.with_namespace(delegate.as_key().into());

            let reference = match repo.find_reference(name) {
                Ok(reference) => reference,
                Err(e) if super::ext::is_not_found_err(&e) => {
                    log::warn!(
                        target: "radicle",
                        "Missing `{name}` while calculating the canonical reference",
                    );
                    continue;
                }
                Err(e) => return Err(CanonicalError::find_reference(name, e)),
            };

            let Some(oid) = reference.target() else {
                log::warn!(target: "radicle", "Missing target for reference `{name}`");
                continue;
            };

            let kind = Self::find_object_for(delegate, oid.into(), repo)?;

            tips.insert(*delegate, (oid.into(), kind));
        }
        Ok(Canonical {
            refname,
            tips,
            rule,
        })
    }

    pub fn find_object_for(
        did: &Did,
        oid: Oid,
        repo: &raw::Repository,
    ) -> Result<CanonicalObject, CanonicalError> {
        match repo.find_object(*oid, None) {
            Ok(object) => object.kind().and_then(CanonicalObject::new).ok_or_else(|| {
                CanonicalError::invalid_object_type(
                    repo.path().to_path_buf(),
                    *did,
                    oid,
                    object.kind(),
                )
            }),
            Err(err) if super::ext::is_not_found_err(&err) => Err(CanonicalError::missing_object(
                repo.path().to_path_buf(),
                *did,
                oid,
                err,
            )),
            Err(err) => Err(CanonicalError::find_object(oid, err)),
        }
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
    pub fn modify_vote(&mut self, did: Did, oid: Oid, kind: CanonicalObject) {
        self.tips.insert(did, (oid, kind));
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
        /// Ensures [`Oid`]s are of the same object type
        enum Objects {
            Commits(NonEmpty<Oid>),
            Tags(NonEmpty<Oid>),
        }

        impl Objects {
            fn new(oid: Oid, kind: CanonicalObject) -> Self {
                match kind {
                    CanonicalObject::Commit => Self::Commits(NonEmpty::new(oid)),
                    CanonicalObject::Tag => Self::Tags(NonEmpty::new(oid)),
                }
            }

            fn insert(mut self, oid: Oid, kind: CanonicalObject) -> Result<Self, CanonicalObject> {
                match self {
                    Objects::Commits(ref mut commits) => match kind {
                        CanonicalObject::Commit => {
                            commits.push(oid);
                            Ok(self)
                        }
                        CanonicalObject::Tag => Err(CanonicalObject::Tag),
                    },
                    Objects::Tags(ref mut tags) => match kind {
                        CanonicalObject::Commit => {
                            tags.push(oid);
                            Ok(self)
                        }
                        CanonicalObject::Tag => Err(CanonicalObject::Commit),
                    },
                }
            }
        }

        let heads = {
            let heads = self
                .tips
                .iter()
                .filter_map(|(did, tip)| (did != candidate).then_some((did, tip)));

            let mut objects = None;

            for (did, (oid, _)) in heads {
                let kind = find_object_for(did, *oid, repo)?;
                let oid = *oid;
                match objects {
                    None => objects = Some(Objects::new(oid, kind)),
                    Some(objs) => {
                        objects = Some(objs.insert(oid, kind).map_err(|expected| {
                            ConvergesError::mismatched_object(
                                repo.path().to_path_buf(),
                                oid,
                                kind,
                                expected,
                            )
                        })?)
                    }
                }
            }

            objects
        };

        match heads {
            None => Ok(true),
            Some(Objects::Tags(_)) => Ok(true),
            Some(Objects::Commits(heads)) => {
                for head in heads {
                    let (ahead, behind) = repo
                        .graph_ahead_behind(**commit, *head)
                        .map_err(|err| ConvergesError::graph_descendant(*commit, head, err))?;
                    if ahead * behind == 0 {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    fn quorum_tag(&self) -> Result<Oid, QuorumError> {
        let voting = TagVoting::from_targets(
            self.tips
                .values()
                .filter_map(|(commit, kind)| kind.is_tag().then_some(*commit)),
        );
        let mut votes = voting.votes();

        // Keep tags which pass the threshold.
        votes.votes_past_threshold(self.threshold());

        if votes.number_of_candidates() > 1 {
            return Err(QuorumError::DivergingTags {
                refname: self.refname.to_string(),
                threshold: self.threshold(),
                candidates: votes.candidates().cloned().collect(),
            });
        }

        let tag = votes
            .pop_first_candidate()
            .ok_or(QuorumError::NoCandidates {
                refname: self.refname.to_string(),
                threshold: self.threshold(),
            })?;

        Ok((*tag).into())
    }

    /// Computes the quorum or "canonical" tip based on the tips, of `Canonical`,
    /// and the threshold. This can be described as the latest commit that is
    /// included in at least `threshold` histories. In case there are multiple tips
    /// passing the threshold, and they are divergent, an error is returned.
    ///
    /// Also returns an error if `heads` is empty or `threshold` cannot be
    /// satisified with the number of heads given.
    fn quorum_commit(&self, repo: &raw::Repository) -> Result<Oid, QuorumError> {
        let mut voting = CommitVoting::from_targets(
            self.tips
                .values()
                .filter_map(|(commit, kind)| kind.is_commit().then_some(*commit)),
        );
        while let Some(targets) = voting.next_candidate() {
            for (candidate, other) in targets {
                let base = Oid::from(repo.merge_base(*candidate, *other)?);
                voting.found_merge_base(MergeBase {
                    candidate,
                    other,
                    base,
                });
            }
        }
        let mut votes = voting.votes();

        // Keep commits which pass the threshold.
        votes.votes_past_threshold(self.threshold());

        let mut longest = votes
            .pop_first_candidate()
            .ok_or(QuorumError::NoCandidates {
                refname: self.refname.to_string(),
                threshold: self.threshold(),
            })?;

        // Now that all scores are calculated, figure out what is the longest branch
        // that passes the threshold. In case of divergence, return an error.
        for head in votes.candidates() {
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
}

/// Keep track of [`Votes`] for quorums involving tag objects.
struct TagVoting {
    votes: Votes,
}

impl TagVoting {
    fn from_targets(targets: impl Iterator<Item = Oid>) -> Self {
        let votes = targets.fold(Votes::default(), |mut votes, oid| {
            votes.vote(oid);
            votes
        });
        Self { votes }
    }

    fn votes(self) -> Votes {
        self.votes
    }
}

/// Keep track of [`Votes`] for quorums involving commit objects.
///
/// Build a list of candidate commits and count how many "votes" each of them
/// has. Commits get a point for each direct vote, as well as for being part of
/// the ancestry of a commit given to this function.
#[derive(Debug)]
struct CommitVoting {
    candidates: Vec<(Oid, Vec<Oid>)>,
    votes: Votes,
}

impl CommitVoting {
    /// Build the initial set voting.
    fn from_targets(targets: impl Iterator<Item = Oid> + Clone) -> Self {
        let ts = targets.clone();
        let (candidates, votes) = targets.enumerate().fold(
            (Vec::new(), Votes::default()),
            |(mut candidates, mut votes), (i, oid)| {
                candidates.push((oid, ts.clone().skip(i + 1).collect()));
                votes.vote(oid);
                (candidates, votes)
            },
        );
        Self { candidates, votes }
    }

    /// Get the next candidate to be considered for ancestry votes.
    ///
    /// The first of each pair will be the candidate commit, which should be
    /// compared to the other commit to see what their common merge base is. The
    /// merge base is then recorded using [`MergeBase`] and is recorded using
    /// [`CommitVoting::found_merge_base`].
    fn next_candidate(&mut self) -> Option<impl Iterator<Item = (Oid, Oid)>> {
        self.candidates
            .pop()
            .map(|(oid, others)| others.into_iter().map(move |other| (oid, other)))
    }

    /// Record a merge base, and add to the vote if necessary.
    fn found_merge_base(
        &mut self,
        MergeBase {
            candidate,
            other,
            base,
        }: MergeBase,
    ) {
        // Avoid double counting the same commits
        let is_same = candidate == other;
        if !is_same && (base == candidate || base == other) {
            self.votes.vote(base);
        }
    }

    /// Finish the voting process and get the [`Votes`] from the
    /// [`CommitVoting`].
    fn votes(self) -> Votes {
        self.votes
    }
}

/// Record a merge base between `candidate` and `other`.
struct MergeBase {
    /// The candidate commit for the merge base.
    candidate: Oid,
    /// The commit that is being compared against for the merge base.
    other: Oid,
    /// The computed merge base commit.
    base: Oid,
}

/// Count the number of votes per [`Oid`].
///
/// Note that the count cannot exceed 255, since that is the maximum number the
/// `threshold` value can be.
#[derive(Debug, Default, PartialEq, Eq)]
struct Votes {
    inner: BTreeMap<Oid, u8>,
}

impl Votes {
    /// Increase the vote count for `oid`.
    ///
    /// If `oid` does not exist in the set of [`Votes`] yet, then no vote will
    /// be added.
    #[inline]
    fn vote(&mut self, oid: Oid) {
        self.safe_inc(oid, 1);
    }

    /// Filter the candidates by the ones that have a number of votes that pass
    /// the `threshold`.
    #[inline]
    fn votes_past_threshold(&mut self, threshold: usize) {
        self.inner.retain(|_, votes| *votes as usize >= threshold);
    }

    /// Get the number of candidates this set of votes has.
    #[inline]
    fn number_of_candidates(&self) -> usize {
        self.inner.len()
    }

    /// Get the set candidates.
    #[inline]
    fn candidates(&self) -> impl Iterator<Item = &Oid> {
        self.inner.keys()
    }

    /// Pop off the first candidate from the set of votes.
    #[inline]
    fn pop_first_candidate(&mut self) -> Option<Oid> {
        self.inner.pop_first().map(|(oid, _)| oid)
    }

    #[inline]
    fn safe_inc(&mut self, oid: Oid, n: u8) {
        let votes = self.inner.entry(oid).or_default();
        *votes = votes.saturating_add(n);
    }
}

fn find_object_for(
    did: &Did,
    oid: Oid,
    repo: &raw::Repository,
) -> Result<CanonicalObject, FindObjectError> {
    match repo.find_object(*oid, None) {
        Ok(object) => object.kind().and_then(CanonicalObject::new).ok_or_else(|| {
            FindObjectError::invalid_object_type(
                repo.path().to_path_buf(),
                *did,
                oid,
                object.kind(),
            )
        }),
        Err(err) if super::ext::is_not_found_err(&err) => Err(FindObjectError::missing_object(
            repo.path().to_path_buf(),
            *did,
            oid,
            err,
        )),
        Err(err) => Err(FindObjectError::find_object(oid, err)),
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
        let tips: BTreeMap<Did, (Oid, CanonicalObject)> = heads
            .iter()
            .enumerate()
            .map(|(i, head)| {
                let signer = Device::mock_from_seed([(i + 1) as u8; 32]);
                let did = Did::from(signer.public_key());
                let kind = repo
                    .find_object(*head, None)
                    .unwrap()
                    .kind()
                    .and_then(CanonicalObject::new)
                    .unwrap();
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

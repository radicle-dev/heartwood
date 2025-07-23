pub mod error;
use error::*;

mod convergence;
use convergence::Convergence;

mod quorum;
use quorum::{CommitQuorum, CommitQuorumFailure, TagQuorum, TagQuorumFailure};

mod voting;

pub mod effects;
pub mod rules;

pub use rules::{MatchedRule, RawRule, Rules, ValidRule};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::ops::ControlFlow;

use git_ext::ref_format::Namespaced;

use crate::prelude::Did;

use super::{Oid, Qualified};

/// A marker for the initial state of [`Canonical`], after construction using
/// [`Canonical::new`].
pub struct Initial;

/// A marker for the state of [`Canonical`] once it has found objects for the
/// calculation, using [`Canonical::find_objects`].
pub struct ObjectsFound;

/// [`Canonical`] captures the state for finding the quorum between a set of
/// [`Did`]s and their references, attempting to agree on a Git commit or tag.
///
/// Construction happens through [`Canonical::new`], at which point you must use
/// [`Canonical::find_objects`] so that the state has the set of [`Object`]s it
/// must use for the quorum calculation.
///
/// At this point, the caller can either call [`Canonical::quorum`] to find the
/// quorum, or modify the calculation by produce a [`CanonicalWithConvergence`]
/// using [`Canonical::with_convergence`], and then using
/// [`CanonicalWithConvergence::quorum`].
pub struct Canonical<'a, 'b, 'r, R, T> {
    refname: Qualified<'a>,
    rule: &'b ValidRule,
    repo: &'r R,
    objects: BTreeMap<Did, Object>,
    missing: Missing,
    _marker: PhantomData<T>,
}

impl<'a, 'b, 'r, R, T> fmt::Debug for Canonical<'a, 'b, 'r, R, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Canonical")
            .field("refname", &self.refname)
            .field("rule", &self.rule)
            .field("objects", &self.objects)
            .field("missing", &self.missing)
            .finish()
    }
}

/// Similar to [`Canonical`], however it checks that a new vote of a single
/// [`Did`] converges with any other [`Did`], to then see if that provides a
/// different quorum result.
pub struct CanonicalWithConvergence<'a, 'b, 'r, R> {
    canonical: Canonical<'a, 'b, 'r, R, ObjectsFound>,
    convergence: Convergence<'r, R>,
}

impl<'a, 'b, 'r, R> fmt::Debug for CanonicalWithConvergence<'a, 'b, 'r, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CanonicalWithConvergence")
            .field("canonical", &self.canonical)
            .field("convergence", &self.convergence)
            .finish()
    }
}

impl<'a, 'b, 'r, R> AsRef<Canonical<'a, 'b, 'r, R, ObjectsFound>>
    for CanonicalWithConvergence<'a, 'b, 'r, R>
{
    fn as_ref(&self) -> &Canonical<'a, 'b, 'r, R, ObjectsFound> {
        &self.canonical
    }
}

impl<'a, 'b, 'r, R> Canonical<'a, 'b, 'r, R, Initial>
where
    R: effects::Ancestry + effects::FindMergeBase + effects::FindObjects,
{
    /// Construct a new [`Canonical`] with the given [`Qualified`] reference, a
    /// canonical reference [`ValidRule`] for that reference, and the Git
    /// repository to load and check the Git data.
    pub fn new(refname: Qualified<'a>, rule: &'b ValidRule, repo: &'r R) -> Self {
        Self {
            refname,
            rule,
            repo,
            missing: Missing::default(),
            objects: BTreeMap::new(),
            _marker: PhantomData,
        }
    }

    /// Find the objects for the [`Canonical`] computation, and prepare it to
    /// find the quorum.
    pub fn find_objects(
        self,
    ) -> Result<Canonical<'a, 'b, 'r, R, ObjectsFound>, effects::FindObjectsError> {
        let FoundObjects {
            objects,
            missing_refs,
            missing_objects,
        } = self
            .repo
            .find_objects(&self.refname, self.rule.allowed().iter())?;
        let missing = Missing {
            refs: missing_refs,
            objects: missing_objects,
        };
        Ok(Canonical {
            refname: self.refname,
            rule: self.rule,
            repo: self.repo,
            missing,
            objects,
            _marker: PhantomData,
        })
    }
}

impl<'a, 'b, 'r, R> Canonical<'a, 'b, 'r, R, ObjectsFound>
where
    R: effects::Ancestry + effects::FindMergeBase + effects::FindObjects,
{
    /// Adds the check for convergence before finding the quorum.
    pub fn with_convergence(
        self,
        candidate: Did,
        object: Object,
    ) -> CanonicalWithConvergence<'a, 'b, 'r, R> {
        let convergence = Convergence::new(self.repo, candidate, object);
        CanonicalWithConvergence {
            canonical: self,
            convergence,
        }
    }

    /// Find the [`Quorum`] for the canonical computation.
    pub fn quorum(self) -> Result<Quorum<'a>, QuorumError> {
        let mut finder = QuorumFinder::new(self.refname, self.rule, self.objects.values());
        while let ControlFlow::Continue(pairs) = finder.find_merge_bases() {
            let mut bases = Vec::with_capacity(pairs.size_hint().0);
            for (a, b) in pairs {
                bases.push(self.repo.merge_base(a, b)?);
            }
            finder.found_merge_bases(bases.into_iter());
        }
        let refname = finder.refname.clone();
        let threshold = (*finder.rule.threshold()).into();
        let results = finder.find_quorum();
        match results {
            (Ok(commit), Err(_)) => Ok(commit),
            (Err(_), Ok(tag)) => Ok(tag),
            (Ok(_), Ok(_)) => Err(QuorumError::DifferentTypes {
                refname: refname.to_string(),
            }),
            (Err(ec), Err(eq)) => Err(Self::convert_failures(
                ec,
                eq,
                refname.to_string(),
                threshold,
            )),
        }
    }

    /// If there are [`Missing`] objects, these may be reported by the caller,
    /// and if further objects are found, then these can be marked using
    /// [`Canonical::found_objects`].
    pub fn missing(&self) -> &Missing {
        &self.missing
    }

    /// Mark the `objects` provided as found, removing them from the [`Missing`]
    /// set.
    pub fn found_objects(&mut self, objects: impl IntoIterator<Item = (Did, Object)>) {
        let objects = objects.into_iter();
        for (did, object) in objects {
            self.missing.found(&did, &self.refname);
            self.objects.insert(did, object);
        }
    }

    fn convert_failures(
        commit: CommitQuorumFailure,
        tag: TagQuorumFailure,
        refname: String,
        threshold: usize,
    ) -> QuorumError {
        match (commit, tag) {
            (CommitQuorumFailure::NoCandidates, TagQuorumFailure::NoCandidates) => {
                QuorumError::NoCandidates { refname, threshold }
            }
            (CommitQuorumFailure::NoCandidates, TagQuorumFailure::DivergingTags { candidates }) => {
                QuorumError::DivergingTags {
                    refname,
                    threshold,
                    candidates,
                }
            }
            (
                CommitQuorumFailure::DivergingCommits {
                    base,
                    longest,
                    candidate,
                },
                _,
            ) => QuorumError::DivergingCommits {
                refname,
                threshold,
                base,
                longest,
                head: candidate,
            },
            (CommitQuorumFailure::NoMergeBase { a, b }, _) => {
                #[derive(thiserror::Error, Debug)]
                #[error("no existing merge base found for commit quorum")]
                struct NoMergeBase;

                effects::MergeBaseError::new(a, b, NoMergeBase).into()
            }
        }
    }
}

impl<'a, 'b, 'r, R> CanonicalWithConvergence<'a, 'b, 'r, R>
where
    R: effects::Ancestry + effects::FindMergeBase + effects::FindObjects,
{
    /// Find the [`QuorumWithConvergence`] for the canonical computation.
    pub fn quorum(mut self) -> Result<QuorumWithConvergence<'a>, QuorumError> {
        let converges = match self.convergence.check(self.canonical.objects.iter())? {
            Some((candidate, object)) => {
                if self.canonical.objects.is_empty()
                    || self.canonical.rule.allowed().is_only(&candidate)
                {
                    self.canonical.objects.insert(candidate, object);
                }
                true
            }
            None => false,
        };
        let quorum = self.canonical.quorum()?;
        Ok(QuorumWithConvergence { quorum, converges })
    }
}

/// The result of finding a quorum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quorum<'a> {
    /// The reference the quorum has been found for.
    pub refname: Qualified<'a>,
    /// The object the reference should be updated to.
    pub object: Object,
}

/// Similar to [`Quorum`], but also reports whether the candidate converged with
/// one of the other voters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuorumWithConvergence<'a> {
    pub quorum: Quorum<'a>,
    pub converges: bool,
}

/// Helper to perform the quorum check for both a [`TagQuorum`] and
/// [`CommitQuorum`].
#[derive(Debug)]
struct QuorumFinder<'a, 'b> {
    refname: Qualified<'a>,
    rule: &'b ValidRule,
    tag_quorum: TagQuorum,
    commit_quorum: CommitQuorum,
}

impl<'a, 'b> QuorumFinder<'a, 'b> {
    fn new<'c, I>(refname: Qualified<'a>, rule: &'b ValidRule, objects: I) -> Self
    where
        I: Iterator<Item = &'c Object> + Clone,
    {
        let threshold = *rule.threshold();
        let tag_quorum = TagQuorum::new(objects.clone(), threshold.into());
        let commit_quorum = CommitQuorum::new(objects, threshold.into());
        Self {
            refname,
            rule,
            tag_quorum,
            commit_quorum,
        }
    }

    fn find_merge_bases(&mut self) -> ControlFlow<(), impl Iterator<Item = (Oid, Oid)>> {
        match self.commit_quorum.next_candidate() {
            Some(candidate) => ControlFlow::Continue(candidate),
            None => ControlFlow::Break(()),
        }
    }

    fn found_merge_bases<I>(&mut self, bases: I)
    where
        I: Iterator<Item = MergeBase>,
    {
        self.commit_quorum.found_merge_bases(bases);
    }

    fn find_quorum(
        self,
    ) -> (
        Result<Quorum<'a>, quorum::CommitQuorumFailure>,
        Result<Quorum<'a>, quorum::TagQuorumFailure>,
    ) {
        let commit = self.commit_quorum.find_quorum().map(|id| Quorum {
            refname: self.refname.clone(),
            object: Object::Commit { id },
        });
        let tag = self.tag_quorum.find_quorum().map(|id| Quorum {
            refname: self.refname.clone(),
            object: Object::Tag { id },
        });
        (commit, tag)
    }
}

/// Record a merge base between `a` and `b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeBase {
    /// The first commit for the merge base.
    pub a: Oid,
    /// The second commit that is being compared against for the merge base.
    pub b: Oid,
    /// The computed merge base commit.
    pub base: Oid,
}

impl MergeBase {
    /// The merge base of the same commit is the commit itself.
    #[cfg(test)]
    pub fn trivial(oid: Oid) -> Self {
        Self {
            a: oid,
            b: oid,
            base: oid,
        }
    }

    /// The result of a merge base computation is trivial.
    pub fn is_trivial(&self) -> bool {
        if self.a == self.b {
            // By definition, the merge base of a commit and itself is itself.
            // These asserts will catch our fall in case we set an invalid
            // base in this case.
            debug_assert_eq!(self.a, self.base);
            debug_assert_eq!(self.b, self.base);
            true
        } else {
            false
        }
    }

    /// Collapses a non-trivial and linear result of a merge base computation
    /// into a single commit, if possible.
    pub fn linear(self) -> Option<Oid> {
        if self.is_trivial() || (self.a != self.base && self.b != self.base) {
            None
        } else {
            Some(self.base)
        }
    }
}

/// The supported type of Git object and its [`Oid`], for canonical computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Object {
    Commit { id: Oid },
    Tag { id: Oid },
}

impl Object {
    pub fn new(obj: &crate::git::raw::Object) -> Option<Self> {
        obj.kind().and_then(|kind| match kind {
            crate::git::raw::ObjectType::Commit => Some(Self::Commit {
                id: obj.id().into(),
            }),
            crate::git::raw::ObjectType::Tag => Some(Self::Tag {
                id: obj.id().into(),
            }),
            _ => None,
        })
    }

    /// The [`Oid`] of the [`Object`]
    pub fn id(&self) -> Oid {
        match self {
            Object::Commit { id } => *id,
            Object::Tag { id } => *id,
        }
    }

    /// Checks if the object is a Git commit.
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Commit { .. })
    }

    /// Checks if the object is a Git tag.
    pub fn is_tag(&self) -> bool {
        matches!(self, Self::Commit { .. })
    }

    /// Returns the [`ObjectType`] of the [`Object`].
    pub fn object_type(&self) -> ObjectType {
        match self {
            Object::Commit { .. } => ObjectType::Commit,
            Object::Tag { .. } => ObjectType::Tag,
        }
    }
}

/// Supported Git object types for canonical computation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType {
    /// The Git object corresponds to a commit.
    Commit,
    /// The Git object corresponds to a tag.
    Tag,
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectType::Commit => f.write_str("commit"),
            ObjectType::Tag => f.write_str("tag"),
        }
    }
}

/// The result of checking the relationship between two commits in the commit graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GraphAheadBehind {
    /// The number of commits the given commit is ahead of the other.
    pub ahead: usize,
    /// The number of commits the given commit is behind the other.
    pub behind: usize,
}

impl GraphAheadBehind {
    /// Whether self represents a linear history between two commits.
    ///
    /// The following three conditions are equivalent characterizations of
    /// a linear history:
    ///  1. One commit is ahead and not behind of the other.
    ///  2. One commit is behind and not ahead of the other.
    ///  3. One commit can be "fast-forwarded" to the other.
    pub fn is_linear(&self) -> bool {
        self.ahead * self.behind == 0
    }
}

/// The result of finding a set of objects in a Git repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundObjects {
    /// The found objects, and under which [`Did`] they were found.
    pub objects: BTreeMap<Did, Object>,
    /// Any missing references while attempting to find the objects.
    pub missing_refs: BTreeSet<Namespaced<'static>>,
    // TODO(finto): I think this doesn't make sense now that we use only one
    // repository.
    /// Any missing objects, where the reference was found, but the object was
    /// missing.
    pub missing_objects: BTreeMap<Did, Oid>,
}

/// [`Missing`] marks whether there were any missing references or objects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Missing {
    pub refs: BTreeSet<Namespaced<'static>>,
    pub objects: BTreeMap<Did, Oid>,
}

impl Missing {
    fn found<'a>(&mut self, did: &Did, refname: &Qualified<'a>) {
        self.objects.remove(did);
        self.refs
            .remove(&refname.with_namespace((did.as_key()).into()).to_owned());
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
        let refname =
            git::refs::branch(git_ext::ref_format::RefStr::try_from_str("master").unwrap());

        let mut delegates = Vec::new();
        for (i, head) in heads.iter().enumerate() {
            let signer = Device::mock_from_seed([(i + 1) as u8; 32]);
            let did = Did::from(signer.public_key());
            delegates.push(did);
            let ns = git::Component::from(signer.public_key());
            repo.reference(refname.with_namespace(ns).as_str(), *head, true, "")
                .unwrap();
        }

        let rule: RawRule = crate::git::canonical::rules::Rule::new(
            crate::git::canonical::rules::Allowed::Delegates,
            threshold,
        );
        let delegates = crate::identity::doc::Delegates::new(delegates).unwrap();
        let rule = rule.validate(&mut || delegates.clone()).unwrap();

        Canonical::new(refname, &rule, repo)
            .find_objects()
            .unwrap()
            .quorum()
            .map(|Quorum { object, .. }| object.id())
    }

    fn commit(id: &str) -> Object {
        Object::Commit {
            id: id.parse().unwrap(),
        }
    }

    fn tag(id: &str) -> Object {
        Object::Tag {
            id: id.parse().unwrap(),
        }
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
    fn test_quorum_different_types() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, c0) = fixtures::repository(tmp.path());
        let c0: git::Oid = c0.into();
        let t0 = fixtures::tag("v1", "", *c0, &repo);

        assert_matches!(
            quorum(&[*c0, *t0], 1, &repo),
            Err(QuorumError::DifferentTypes { .. })
        );
    }

    #[test]
    fn test_commit_quorum_groups() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");

        //   C1  C2
        //    \ /
        //     C0

        let mut cq = CommitQuorum::new([c1, c2, c1, c2].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: c1.id(),
            b: c2.id(),
            base: c0.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([c1, c2].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: c1.id(),
            b: c2.id(),
            base: c0.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );
    }

    #[test]
    fn test_tag_quorum() {
        let t1 = tag("0480391dd7312d35c79a455ec5d004657260b358");
        let t2 = tag("a2eec713ec5c287ecdf13a0180f68acfef7962d0");

        assert_eq!(
            TagQuorum::new([t1].iter(), 1).find_quorum().unwrap(),
            t1.id()
        );
        assert_eq!(
            TagQuorum::new([t1, t1].iter(), 2).find_quorum().unwrap(),
            t1.id()
        );
        assert_matches!(
            TagQuorum::new([t1, t2].iter(), 1).find_quorum(),
            Err(TagQuorumFailure::DivergingTags { .. })
        );
    }

    #[test]
    fn test_commit_quorum_single() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        assert_eq!(
            CommitQuorum::new([c0].iter(), 1).find_quorum().unwrap(),
            c0.id()
        );
        assert_eq!(
            CommitQuorum::new([c1].iter(), 1).find_quorum().unwrap(),
            c1.id()
        );
        assert_eq!(
            CommitQuorum::new([c2].iter(), 1).find_quorum().unwrap(),
            c2.id()
        );
    }

    #[test]
    fn test_commit_quorum_linear() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        //   C2
        //   |
        //  C1
        //  |
        // C0
        let merge_bases = [
            MergeBase {
                a: c2.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c1.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase::trivial(c2.id()),
            MergeBase::trivial(c1.id()),
            MergeBase::trivial(c0.id()),
        ];

        let mut cq = CommitQuorum::new([c1, c2].iter(), 1);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c2.id());

        let mut cq = CommitQuorum::new([c1, c2].iter(), 2);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([c0, c1, c2].iter(), 3);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c0.id());

        let mut cq = CommitQuorum::new([c1, c1, c2].iter(), 2);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([c1, c1, c2].iter(), 1);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c2.id());

        let mut cq = CommitQuorum::new([c2, c2, c1].iter(), 1);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c2.id());
    }

    #[test]
    fn test_commit_quorum_two_way_fork() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        let b2 = commit("037a148170e3d41524b7c482a4798e5c2daeaa00");

        eprintln!("C0: {}", c0.id());
        eprintln!("C1: {}", c1.id());
        eprintln!("C2: {}", c2.id());
        eprintln!("B2: {}", b2.id());

        // B2 C2
        //   \|
        //   C1
        //   |
        //  C0
        let merge_bases = [
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c1.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase::trivial(b2.id()),
            MergeBase::trivial(c2.id()),
            MergeBase::trivial(c1.id()),
            MergeBase::trivial(c0.id()),
        ];

        let mut cq = CommitQuorum::new([c1, c2, b2].iter(), 1);
        cq.found_merge_bases(merge_bases);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([c2, b2].iter(), 1);
        cq.found_merge_bases(merge_bases);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([b2, c2].iter(), 1);
        cq.found_merge_bases(merge_bases);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        // Note for the next two cases we only give enough merge base
        // information so that the quorum fails. If we provided all
        // `merge_bases`, it would mean that c0 could be chosen as the quourum.
        let mut cq = CommitQuorum::new([c2, b2].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: b2.id(),
            b: c2.id(),
            base: c1.id(),
        }]);
        assert_matches!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([b2, c2].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: b2.id(),
            b: c2.id(),
            base: c1.id(),
        }]);
        assert_matches!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([c1, c2, b2].iter(), 2);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([c1, c2, b2].iter(), 3);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([b2, b2, c2].iter(), 2);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), b2.id());

        let mut cq = CommitQuorum::new([b2, c2, c2].iter(), 2);
        cq.found_merge_bases(merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c2.id());

        let mut cq = CommitQuorum::new([b2, b2, c2, c2].iter(), 2);
        cq.found_merge_bases(merge_bases);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );
    }

    #[test]
    fn test_commit_quorum_three_way_fork() {
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        let c3 = commit("07c2a0f856e0d6b08115f98a265df88c4e507fa0");
        let b2 = commit("037a148170e3d41524b7c482a4798e5c2daeaa00");

        // B2 C2 C3
        //  \ | /
        //    C1
        //    |
        //    C0
        let mut cq = CommitQuorum::new([b2, c2, c2].iter(), 2);
        cq.found_merge_bases([
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase::trivial(b2.id()),
        ]);
        assert_eq!(cq.find_quorum().unwrap(), c2.id());

        let mut cq = CommitQuorum::new([b2, c2, c2].iter(), 3);
        cq.found_merge_bases([
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase::trivial(c2.id()),
        ]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([b2, c2, b2, c2].iter(), 3);
        cq.found_merge_bases([
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase::trivial(b2.id()),
            MergeBase::trivial(c2.id()),
        ]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([c3, b2, c2, b2, c2, c3].iter(), 3);
        cq.found_merge_bases([
            MergeBase {
                a: c3.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c3.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase::trivial(b2.id()),
            MergeBase::trivial(c2.id()),
            MergeBase::trivial(c3.id()),
        ]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));
    }

    #[test]
    fn test_commit_quorum_fork_of_a_fork() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        let b2 = commit("037a148170e3d41524b7c482a4798e5c2daeaa00");
        let a1 = commit("2224468e22b30359611d880ccf0850d023f86f9b");

        //  B2 C2
        //    \|
        // A1 C1
        //   \|
        //   C0
        let mut cq = CommitQuorum::new([c2, b2, a1].iter(), 1);
        cq.found_merge_bases([
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: a1.id(),
                base: c0.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );
        let mut cq = CommitQuorum::new([c2, b2, a1].iter(), 2);
        cq.found_merge_bases([
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: a1.id(),
                base: c0.id(),
            },
        ]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([c2, b2, a1].iter(), 3);
        cq.found_merge_bases([
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: a1.id(),
                base: c0.id(),
            },
        ]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([c1, c2, b2, a1].iter(), 4);
        cq.found_merge_bases([
            MergeBase {
                a: c1.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c1.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c1.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: a1.id(),
                base: c0.id(),
            },
        ]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let all_merge_bases = [
            MergeBase {
                a: c0.id(),
                b: c1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c0.id(),
                b: c2.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c0.id(),
                b: b2.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c0.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c1.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c1.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c1.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c2.id(),
                b: b2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: c2.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: a1.id(),
                base: c0.id(),
            },
        ];
        let mut cq = CommitQuorum::new([c0, c1, c2, b2, a1].iter(), 2);
        cq.found_merge_bases(all_merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([c0, c1, c2, b2, a1].iter(), 3);
        cq.found_merge_bases(all_merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([c0, c2, b2, a1].iter(), 3);
        cq.found_merge_bases(all_merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c0.id());

        let mut cq = CommitQuorum::new([c0, c1, c2, b2, a1].iter(), 4);
        cq.found_merge_bases(all_merge_bases);
        assert_eq!(cq.find_quorum().unwrap(), c0.id());

        let mut cq = CommitQuorum::new([a1, a1, c2, c2, c1].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(a1.id()),
            MergeBase::trivial(c2.id()),
            MergeBase {
                a: a1.id(),
                b: c2.id(),
                base: c0.id(),
            },
            MergeBase {
                a: a1.id(),
                b: c1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c2.id(),
                b: c1.id(),
                base: c0.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([a1, a1, c2, c2, c1].iter(), 1);
        cq.found_merge_bases([
            MergeBase::trivial(a1.id()),
            MergeBase::trivial(c2.id()),
            MergeBase {
                a: a1.id(),
                b: c2.id(),
                base: c0.id(),
            },
            MergeBase {
                a: a1.id(),
                b: c1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c2.id(),
                b: c1.id(),
                base: c0.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([a1, a1, c2, c2, c1].iter(), 1);
        cq.found_merge_bases([
            MergeBase::trivial(a1.id()),
            MergeBase {
                a: a1.id(),
                b: c2.id(),
                base: c0.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([b2, b2, c2, c2].iter(), 1);
        cq.found_merge_bases([
            MergeBase::trivial(b2.id()),
            MergeBase::trivial(c2.id()),
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([b2, b2, c2, c2, a1].iter(), 1);
        cq.found_merge_bases([
            MergeBase::trivial(b2.id()),
            MergeBase::trivial(c2.id()),
            MergeBase {
                a: b2.id(),
                b: c2.id(),
                base: c1.id(),
            },
            MergeBase {
                a: b2.id(),
                b: a1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c2.id(),
                b: a1.id(),
                base: c0.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );
    }

    #[test]
    fn test_commit_quorum_forked_merge_commits() {
        let c0 = commit("f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354");
        let c1 = commit("bfb1a513e420eade90b0e6be64117b861b16ecb5");
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        let b2 = commit("037a148170e3d41524b7c482a4798e5c2daeaa00");
        let a1 = commit("2224468e22b30359611d880ccf0850d023f86f9b");
        let m1 = commit("dd7ee5bca2fc7288a6efcb4303278e26a2dbaa45");
        let m2 = commit("d54e505e3fb5c0c7e4b9a4b8a1cdeefb3fc9ef18");

        //    M2  M1
        //    /\  /\
        //    \ B2 C2
        //     \  \|
        //     A1 C1
        //       \|
        //       C0
        let cq = CommitQuorum::new([m1].iter(), 1);
        assert_eq!(cq.find_quorum().unwrap(), m1.id());

        let mut cq = CommitQuorum::new([m1, m2].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: m2.id(),
            base: b2.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m2, m1].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m2.id(),
            b: m1.id(),
            base: b2.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m1, m2].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: m2.id(),
            base: b2.id(),
        }]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([m1, m2, c2].iter(), 1);
        cq.found_merge_bases([
            MergeBase {
                a: m1.id(),
                b: m2.id(),
                base: b2.id(),
            },
            MergeBase {
                a: m1.id(),
                b: c2.id(),
                base: c2.id(),
            },
            MergeBase {
                a: m2.id(),
                b: c2.id(),
                base: c0.id(),
            },
        ]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m1, a1].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: a1.id(),
            base: c0.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m1, a1].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: a1.id(),
            base: c0.id(),
        }]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([m1, m2, b2, c1].iter(), 4);
        cq.found_merge_bases([
            MergeBase {
                a: m1.id(),
                b: m2.id(),
                base: b2.id(),
            },
            MergeBase {
                a: m1.id(),
                b: b2.id(),
                base: b2.id(),
            },
            MergeBase {
                a: m1.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: m2.id(),
                b: b2.id(),
                base: b2.id(),
            },
            MergeBase {
                a: m2.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c1.id(),
                base: c1.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([m1, m1, b2].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(m1.id()),
            MergeBase {
                a: m1.id(),
                b: b2.id(),
                base: b2.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), m1.id());

        let mut cq = CommitQuorum::new([m1, m1, c2].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(m1.id()),
            MergeBase {
                a: m1.id(),
                b: c2.id(),
                base: c2.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), m1.id());

        let mut cq = CommitQuorum::new([m2, m2, b2].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(m2.id()),
            MergeBase {
                a: m2.id(),
                b: b2.id(),
                base: b2.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), m2.id());

        let mut cq = CommitQuorum::new([m2, m2, a1].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(m2.id()),
            MergeBase {
                a: m2.id(),
                b: a1.id(),
                base: a1.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), m2.id());

        let mut cq = CommitQuorum::new([m1, m1, b2, b2].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(m1.id()),
            MergeBase::trivial(b2.id()),
            MergeBase {
                a: m1.id(),
                b: b2.id(),
                base: b2.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), m1.id());

        let mut cq = CommitQuorum::new([m1, m1, c2, c2].iter(), 2);
        cq.found_merge_bases([
            MergeBase::trivial(m1.id()),
            MergeBase::trivial(c2.id()),
            MergeBase {
                a: m1.id(),
                b: c2.id(),
                base: c2.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), m1.id());

        let mut cq = CommitQuorum::new([m1, b2, c1, c0].iter(), 3);
        cq.found_merge_bases([
            MergeBase {
                a: m1.id(),
                b: b2.id(),
                base: b2.id(),
            },
            MergeBase {
                a: m1.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: m1.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c1.id(),
                b: c0.id(),
                base: c0.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), c1.id());

        let mut cq = CommitQuorum::new([m1, b2, c1, c0].iter(), 4);
        cq.found_merge_bases([
            MergeBase {
                a: m1.id(),
                b: b2.id(),
                base: b2.id(),
            },
            MergeBase {
                a: m1.id(),
                b: c1.id(),
                base: c1.id(),
            },
            MergeBase {
                a: m1.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c1.id(),
                base: c0.id(),
            },
            MergeBase {
                a: b2.id(),
                b: c0.id(),
                base: c0.id(),
            },
            MergeBase {
                a: c1.id(),
                b: c0.id(),
                base: c0.id(),
            },
        ]);
        assert_eq!(cq.find_quorum().unwrap(), c0.id());
    }

    #[test]
    fn test_commit_quorum_merges() {
        let c2 = commit("8fc5160702365f231c77732a8fa162379e54f57a");
        let m1 = commit("dd7ee5bca2fc7288a6efcb4303278e26a2dbaa45");
        let m2 = commit("d54e505e3fb5c0c7e4b9a4b8a1cdeefb3fc9ef18");
        let m3 = commit("2224468e22b30359611d880ccf0850d023f86f9b");

        //    M2  M1
        //    /\  /\
        //   C1 C2 C3
        //     \| /
        //      C0

        let mut cq = CommitQuorum::new([m1, m2].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: m2.id(),
            base: c2.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m1, m2].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: m2.id(),
            base: c2.id(),
        }]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        //   M3/M2 M1
        //    /\  /\
        //   C1 C2 C3
        //     \| /
        //      C0
        let mut cq = CommitQuorum::new([m1, m3].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: m3.id(),
            base: c2.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m1, m3].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: m1.id(),
            b: m3.id(),
            base: c2.id(),
        }]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([m3, m1].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m3.id(),
            b: m1.id(),
            base: c2.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m3, m1].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: m3.id(),
            b: m1.id(),
            base: c2.id(),
        }]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));

        let mut cq = CommitQuorum::new([m3, m2].iter(), 1);
        cq.found_merge_bases([MergeBase {
            a: m3.id(),
            b: m2.id(),
            base: c2.id(),
        }]);
        assert_matches!(
            cq.find_quorum(),
            Err(CommitQuorumFailure::DivergingCommits { .. })
        );

        let mut cq = CommitQuorum::new([m3, m2].iter(), 2);
        cq.found_merge_bases([MergeBase {
            a: m3.id(),
            b: m2.id(),
            base: c2.id(),
        }]);
        assert_eq!(cq.find_quorum(), Err(CommitQuorumFailure::NoCandidates));
    }
}

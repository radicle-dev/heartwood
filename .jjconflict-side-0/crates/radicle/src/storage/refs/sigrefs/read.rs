pub mod error;

mod iter;

#[cfg(test)]
mod test;

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;
use std::path::Path;

use crypto::{PublicKey, signature};
use nonempty::NonEmpty;
use radicle_core::{NodeId, RepoId};
use radicle_git_metadata::commit::CommitData;
use radicle_oid::Oid;

use crate::git;
use crate::identity::doc;
use crate::storage::refs::sigrefs::git::{object, reference};
use crate::storage::refs::{
    FeatureLevel, IDENTITY_ROOT, REFS_BLOB_PATH, Refs, SIGNATURE_BLOB_PATH, SIGREFS_BRANCH,
    SignedRefs,
};

/// A `rad/sigrefs` that has passed the following verification checks:
///
/// - Has a valid `/signature` blob, which is verified by the signing key.
/// - Contains the `refs/rad/root` entry under `/refs`, which matches the
///   [`RepoId`] of the local repository.
/// - The `refs/rad/sigrefs-parent` entry matches the commit's parent, if the
///   entry exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCommit {
    /// The commit that was verified.
    commit: Commit,
    /// The feature level that was recognized for the commit that was verified.
    level: FeatureLevel,
}

impl VerifiedCommit {
    /// Borrow the [`Commit`] that was verified.
    pub(super) fn commit(&self) -> &Commit {
        &self.commit
    }

    // The [`FeatureLevel`] of the refs in this commit.
    #[cfg(test)]
    pub(super) fn level(&self) -> FeatureLevel {
        self.level
    }

    pub(crate) fn into_sigrefs_at(self, id: PublicKey) -> SignedRefs {
        SignedRefs {
            refs: self.commit.refs,
            signature: self.commit.signature,
            id,
            level: self.level,
            parent: self.commit.parent,
            at: self.commit.oid,
        }
    }
}

/// A [`SignedRefsReader`] reads and verifies a commit chain for a `rad/sigrefs`
/// entry.
///
/// To create a new reader, use [`SignedRefsReader::new`].
///
/// The construction expects:
/// - A [`RepoId`] which is the repository identifier of the Radicle repository.
/// - A [`Tip`] which describes where and how to start the verification.
/// - A `repository` which is the Git repository that is being used for the reading.
/// - A `verifier` which is the entity that verifies the cryptographic signatures.
pub struct SignedRefsReader<'a, R, V> {
    rid: RepoId,
    tip: Tip,
    repository: &'a R,
    verifier: &'a V,
}

/// Describe where to start a [`SignedRefsReader`]'s commit chain.
pub enum Tip {
    /// Use the namespace of the given [`NodeId`], resolving their `rad/sigrefs`
    /// to its commit [`Oid`].
    Reference(NodeId),
    /// Use the supplied commit [`Oid`].
    Commit(Oid),
}

/// Describes the feature levels of a history of commits.
#[derive(Debug, PartialEq)]
pub struct FeatureLevels(BTreeMap<FeatureLevel, Oid>);

impl FeatureLevels {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn max(&self) -> FeatureLevel {
        self.0.last_key_value().map(|(k, _)| *k).unwrap_or_default()
    }

    fn insert(&mut self, verified: &VerifiedCommit) {
        if verified.level != FeatureLevel::None {
            self.0.entry(verified.level).or_insert(verified.commit.oid);
        }
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test(from: impl IntoIterator<Item = (FeatureLevel, Oid)>) -> Self {
        Self(BTreeMap::from_iter(from))
    }
}

impl std::fmt::Display for FeatureLevels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            &self
                .0
                .iter()
                .map(|(level, at)| format!("{level}@{at}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl<'a, R, V> SignedRefsReader<'a, R, V>
where
    R: object::Reader + reference::Reader,
    V: signature::Verifier<crypto::Signature>,
{
    /// Construct a new [`SignedRefsReader`].
    pub fn new(rid: RepoId, tip: Tip, repository: &'a R, verifier: &'a V) -> Self {
        Self {
            rid,
            tip,
            repository,
            verifier,
        }
    }

    /// Read a [`VerifiedCommit`] using the [`SignedRefsReader`], from a
    /// linear history.
    ///
    /// The [`VerifiedCommit`] will be the latest commit, if the commit verifies
    /// and contains its parent in its [`Refs`] entry.
    ///
    /// If the commit does not contain a parent, but its signature is not
    /// repeated, then it is still returned.
    ///
    /// Otherwise, the latest commit that has no duplicate signatures in its
    /// ancestry is returned.
    ///
    /// # Replay Attacks
    ///
    /// The [`SignedRefsReader`] prevents replay attacks via two mechanisms:
    /// - The first is recording the parent commit in the `/refs` blob. This
    ///   prevents a replay by not allowing the same signature payload to be
    ///   used in a new commit, since the parents would not match. Note that
    ///   this does not detect replays by older clients, since they will not
    ///   include this entry in `/refs`.
    /// - The second mechanism uses the fact that a replay will give duplicate
    ///   signatures. This means that any repeated signatures will be skipped,
    ///   and the commit returned will be the first valid commit, that was not a
    ///   replay.
    pub fn read(self) -> Result<VerifiedCommit, error::Read> {
        const ONE: NonZeroUsize = NonZeroUsize::new(1).expect("one is non-zero");
        const SIGNATURES_COLLECTED: &str = "all signatures were collected";

        let mut head = CommitReader::new(self.resolve_tip()?, self.repository)
            .read()
            .map_err(error::Read::Commit)?
            .verify(self.rid, self.verifier)
            .map_err(error::Read::Verify)?;

        if head.commit.parent.is_none() && head.level == FeatureLevel::Root {
            head.level = FeatureLevel::Parent;
        }

        let head = head;

        if head.level >= FeatureLevel::Parent {
            // `head` is verified, thus we know that if the parent reference
            // exists, its target actually matches the parent OID.
            // The fact that the parent OID is a hash over all previous history
            // makes it *incredibly unlikely* or rather *practically impossible*
            // that the same `/refs` blob re-appears in previous history.
            // Thus, we can spare oureselves walking the history.
            return Ok(head);
        }

        // `seen` maps from signatures to the `NonEmpty` of commits they were
        // seen in. Note that for all sets of commits which share the same
        // signature, the `NonEmpty` in `seen` will be in reverse order of the
        // walk.  That is, the latest commit in the set will be at the first
        // position, and the earliest commit will be at the last position.
        //
        // `level` is the feature level of the history, which is
        // the maximum feature level over all commits in the history.
        let (seen, levels) = iter::Walk::new(head.commit.oid, self.repository).try_fold(
            (
                HashMap::<crypto::Signature, NonEmpty<Oid>>::new(),
                FeatureLevels::new(),
            ),
            |(mut seen, mut levels), commit| {
                let commit = commit.map_err(error::Read::Commit)?;

                seen.entry(commit.signature)
                    .and_modify(|value| value.push(commit.oid))
                    .or_insert_with(|| NonEmpty::new(commit.oid));

                // Before `commit` can be interpreted for feature detection,
                // it must be verified. In particular, we do not want to
                // detect features on commits that have an invalid signature.
                // However, if we have already reached the maximum level,
                // this is not required anymore, since it cannot increase any
                // further.
                if levels.max() < FeatureLevel::LATEST {
                    let commit = commit
                        .verify(self.rid, self.verifier)
                        .map_err(error::Read::Verify)?;

                    if commit.level > FeatureLevel::None {
                        levels.insert(&commit);
                    }
                }

                Ok((seen, levels))
            },
        )?;

        let level = levels.max();

        if head.level < level {
            return Err(error::Read::Downgrade {
                levels,
                actual: head.level,
                commit: head.commit.oid,
            });
        }

        if seen
            .get(&head.commit.signature)
            .expect(SIGNATURES_COLLECTED)
            .len_nonzero()
            == ONE
        {
            // `head` has a verified, non-repeated signature, but does not
            // include the parent reference in the `/refs` blob. Maintains
            // backwards-compatibility.
            return Ok(head);
        }

        // If the signature in `head` was seen twice, then
        // `head` must have a parent.
        let parent = head.commit.parent.expect("parent must exist");

        // The second walk can start from the parent of `head`. We do not need to
        // verify `head` twice, and we already know that the parent exists.
        for commit in iter::Walk::new(parent, self.repository) {
            let verified = commit
                .map_err(error::Read::Commit)?
                .verify(self.rid, self.verifier)
                .map_err(error::Read::Verify)?;

            if verified.level < level {
                // To avoid downgrade attacks, we skip `commit`.
                continue;
            }

            let commit = verified.commit();

            let commits = seen.get(&commit.signature).expect(SIGNATURES_COLLECTED);
            if commits.len_nonzero() == ONE {
                return Ok(verified);
            }

            let id = &commit.oid;
            if id == commits.last() {
                // If this commit is the last element of `commits`,
                // then this means it is the earliest of all that share
                // its signature. It thus cannot have been replayed.
                return Ok(verified);
            }

            if id == commits.first() {
                // We only log one warning per set of duplicates, and that is
                // when we reach the first element of `commits`, which is the
                // latest in the history.
                log::warn!("Duplicate signature found in commits {commits:?}");
            }
        }

        unreachable!()
    }

    fn resolve_tip(&self) -> Result<Oid, error::Read> {
        match self.tip {
            Tip::Commit(oid) => Ok(oid),
            Tip::Reference(namespace) => {
                let reference =
                    SIGREFS_BRANCH.with_namespace(git::fmt::Component::from(&namespace));
                let head = self
                    .repository
                    .find_reference(&reference)
                    .map_err(error::Read::FindReference)?
                    .ok_or_else(|| error::Read::MissingSigrefs { namespace })?;
                Ok(head)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Commit {
    oid: Oid,
    parent: Option<Oid>,
    refs: Refs,
    signature: crypto::Signature,
    identity_root: Option<IdentityRoot>,
}

impl Commit {
    /// The [`Oid`] of this commit.
    pub(super) fn oid(&self) -> &Oid {
        &self.oid
    }

    /// The parent [`Oid`] of the commit, unless it is the root commit.
    #[cfg(test)]
    pub(super) fn parent(&self) -> Option<&Oid> {
        self.parent.as_ref()
    }

    /// The [`Refs`] found in the blob at `/refs` in in the tree of this commit.
    pub(super) fn refs(&self) -> &Refs {
        &self.refs
    }

    /// The [`crypto::Signature`] found in blob at `/signature` in the tree of the commit.
    #[cfg(test)]
    pub(super) fn signature(&self) -> &crypto::Signature {
        &self.signature
    }

    pub(super) fn verify<V>(
        mut self,
        expected: RepoId,
        verifier: &V,
    ) -> Result<VerifiedCommit, error::Verify>
    where
        V: signature::Verifier<crypto::Signature>,
    {
        verifier
            .verify(&self.refs.canonical(), &self.signature)
            .map_err(error::Verify::Signature)?;

        let level = if let Some(IdentityRoot {
            commit: identity_commit,
            rid,
        }) = self.identity_root
        {
            if rid != expected {
                return Err(error::Verify::MismatchedIdentity {
                    identity_commit,
                    sigrefs_commit: self.oid,
                    expected,
                    found: rid,
                });
            } else {
                FeatureLevel::Root
            }
        } else {
            FeatureLevel::None
        };

        self.refs.remove_sigrefs();

        let level = match (self.parent, self.refs.remove_parent()) {
            (None, None) | (Some(_), None) => {
                // Pattern 1:
                // We are looking at a root commit.
                // This is a special case, as there is no good value
                // for `rad/refs/sigrefs-parent` to target. The zero OID would
                // be a candidate, but it is filtered out in [`Refs`].
                // Upgrading to `FeatureLevel::Parent` is not a good idea
                // either, otherwise any history containing this commit
                // would be at that level from the root onwards.

                // Pattern 2:
                // The ref `refs/rad/sigrefs-parent` is simply absent,
                // we remain on the same feature level.

                level
            }
            (None, Some(actual)) => {
                // We are looking at a root commit.
                // Any target OID is treated as dangling.
                return Err(error::Verify::DanglingParent {
                    sigrefs_commit: self.oid,
                    actual,
                });
            }
            (Some(expected), Some(actual)) if expected == actual => {
                // We have a good value for `refs/rad/sigrefs-parent`, however,
                // as feature levels are monotonic, we also make sure that the
                // earlier check of `refs/rad/root` was positive.
                // In case the prior feature level was not `FeatureLevel::Root`,
                // we can even error early.
                if level == FeatureLevel::Root {
                    FeatureLevel::Parent
                } else {
                    return Err(error::Verify::IdentityRootDowngrade {
                        sigrefs_commit: self.oid,
                    });
                }
            }
            (Some(expected), Some(actual)) => {
                return Err(error::Verify::MismatchedParent {
                    sigrefs_commit: self.oid,
                    expected,
                    actual,
                });
            }
        };

        Ok(VerifiedCommit {
            commit: self,
            level,
        })
    }
}

pub(super) struct CommitReader<'a, R> {
    commit: Oid,
    repository: &'a R,
}

impl<'a, R> CommitReader<'a, R>
where
    R: object::Reader,
{
    pub(super) fn new(commit: Oid, repository: &'a R) -> Self {
        Self { commit, repository }
    }

    pub(super) fn read(self) -> Result<Commit, error::Commit> {
        let commit = self.read_commit_data()?;
        let Tree { refs, signature } = TreeReader::new(self.commit, self.repository)
            .read()
            .map_err(error::Commit::Tree)?;
        let identity_root = IdentityRootReader::new(&refs, self.repository)
            .read()
            .map_err(error::Commit::IdentityRoot)?;
        let parent = Self::get_parent(&commit).transpose()?;

        Ok(Commit {
            oid: self.commit,
            parent,
            refs,
            signature,
            identity_root,
        })
    }

    fn read_commit_data(&self) -> Result<CommitData<Oid, Oid>, error::Commit> {
        let bytes = self
            .repository
            .read_commit(&self.commit)
            .map_err(error::Commit::Read)?
            .ok_or(error::Commit::Missing { oid: self.commit })?;
        CommitData::from_bytes(&bytes).map_err(|err| error::Commit::Parse {
            oid: self.commit,
            source: err,
        })
    }

    /// Extract the single parent [`Oid`] from a [`CommitData`], if any.
    ///
    /// Returns `None` if the commit has no parents (i.e. it is a root commit).
    /// Returns an error if the commit has more than one parent, since the
    /// transparency log is a linear chain.
    fn get_parent(commit: &CommitData<Oid, Oid>) -> Option<Result<Oid, error::Commit>> {
        let NonEmpty {
            head: parent,
            tail: mut rest,
        } = NonEmpty::collect(commit.parents())?;
        if rest.is_empty() {
            Some(Ok(parent))
        } else {
            rest.insert(0, parent);
            let err = error::Commit::TooManyParents(error::Parent { parents: rest });
            Some(Err(err))
        }
    }
}

struct Tree {
    refs: Refs,
    signature: crypto::Signature,
}

struct TreeReader<'a, R> {
    commit: Oid,
    repository: &'a R,
}

impl<'a, R> TreeReader<'a, R>
where
    R: object::Reader,
{
    fn new(commit: Oid, repository: &'a R) -> Self {
        Self { commit, repository }
    }

    fn read(self) -> Result<Tree, error::Tree> {
        let (refs, signature) = self.try_handle_blobs()?;
        let refs = Refs::from_canonical(&refs.bytes).map_err(error::Tree::ParseRefs)?;
        let signature = crypto::Signature::try_from(signature.bytes.as_slice())
            .map_err(error::Tree::ParseSignature)?;
        Ok(Tree { refs, signature })
    }

    /// Fetch the refs blob and signature blob from the repository, returning a
    /// descriptive error if either or both are missing.
    fn try_handle_blobs(&self) -> Result<(object::Blob, object::Blob), error::Tree> {
        let commit = &self.commit;
        let refs_path = Path::new(REFS_BLOB_PATH);
        let sig_path = Path::new(SIGNATURE_BLOB_PATH);

        let refs_bytes = self
            .repository
            .read_blob(commit, refs_path)
            .map_err(error::Tree::Refs)?;
        let sig_bytes = self
            .repository
            .read_blob(commit, sig_path)
            .map_err(error::Tree::Signature)?;

        let result = match (refs_bytes, sig_bytes) {
            (None, None) => Err(error::MissingBlobs::Both {
                commit: *commit,
                refs: refs_path.to_path_buf(),
                signature: sig_path.to_path_buf(),
            }),
            (None, Some(_)) => Err(error::MissingBlobs::Signature {
                commit: *commit,
                path: sig_path.to_path_buf(),
            }),
            (Some(_), None) => Err(error::MissingBlobs::Refs {
                commit: *commit,
                path: refs_path.to_path_buf(),
            }),
            (Some(refs), Some(sig)) => Ok((refs, sig)),
        };

        result.map_err(error::Tree::from)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IdentityRoot {
    commit: Oid,
    rid: RepoId,
}

struct IdentityRootReader<'a, 'b, R> {
    refs: &'a Refs,
    repository: &'b R,
}

impl<'a, 'b, R> IdentityRootReader<'a, 'b, R>
where
    R: object::Reader,
{
    fn new(refs: &'a Refs, repository: &'b R) -> Self {
        Self { refs, repository }
    }

    fn read(self) -> Result<Option<IdentityRoot>, error::IdentityRoot> {
        match self.refs.get(&IDENTITY_ROOT) {
            Some(commit) => self
                .read_blob(&commit)
                .map(|rid| Some(IdentityRoot { commit, rid })),
            None => Ok(None),
        }
    }

    fn read_blob(&self, commit: &Oid) -> Result<RepoId, error::IdentityRoot> {
        let path = Path::new("embeds").join(*doc::PATH);
        let object::Blob { oid, .. } = self
            .repository
            .read_blob(commit, &path)
            .map_err(error::IdentityRoot::Blob)?
            .ok_or_else(|| error::IdentityRoot::MissingIdentity { commit: *commit })?;
        Ok(RepoId::from(oid))
    }
}

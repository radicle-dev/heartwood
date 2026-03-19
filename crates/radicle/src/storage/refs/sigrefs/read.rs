pub mod error;

mod iter;

#[cfg(test)]
mod test;

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;

use crypto::signature;
use nonempty::NonEmpty;
use radicle_core::{NodeId, RepoId};
use radicle_git_metadata::commit::CommitData;
use radicle_oid::Oid;

use crate::git;
use crate::identity::doc;
use crate::storage::refs::sigrefs::git::{object, reference};
use crate::storage::refs::{
    Refs, IDENTITY_ROOT, REFS_BLOB_PATH, SIGNATURE_BLOB_PATH, SIGREFS_BRANCH,
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
    /// Whether verification successfully found the correct
    /// value for [`SIGREFS_PARENT`] in the refs of [`Self::commit`].
    parent: bool,
}

impl VerifiedCommit {
    /// The [`Oid`] of the commit.
    pub fn id(&self) -> &Oid {
        &self.commit.oid
    }

    /// The [`crypto::Signature`] found in the tree of the commit.
    pub fn signature(&self) -> &crypto::Signature {
        &self.commit.signature
    }

    /// The [`Refs`] found in the tree of the commit.
    pub fn into_refs(self) -> Refs {
        self.commit.refs
    }

    /// The parent [`Oid`] of the commit, unless it is the root commit.
    pub fn parent(&self) -> Option<&Oid> {
        self.commit.parent.as_ref()
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

    /// Read a [`VerifiedCommit`] using the [`SignedRefsReader`].
    ///
    /// The [`VerifiedCommit`] will be the first commit, if the commit verifies
    /// and contains its parent in its [`Refs`] entry.
    /// If the commit does not contain a parent, but its signature is not
    /// repeated, then it is still returned.
    /// Otherwise, the commit that is returned is either:
    /// - The first commit which has no repeated signatures, i.e. it has no replay attacks.
    /// - The first commit which is not a replay commit, i.e. the commit that
    ///   replay attacks are based on.
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

        let head = CommitReader::new(self.resolve_tip()?, self.repository)
            .read()
            .map_err(error::Read::Commit)?
            .verify(self.rid, self.verifier)
            .map_err(error::Read::Verify)?;

        #[cfg(not(debug_assertions))]
        if head.parent {
            // `head` is verified, thus we know that if the parent reference
            // exists, its target actually matches the parent OID.
            // The fact that the parent OID is a hash over all previous history
            // makes it *incredibly unlikely* or rather *practically impossible*
            // that the same `/refs` blob re-appears in previous history.
            // Thus, we can spare oureselves walking the history.
            return Ok(head);
        }

        let seen = iter::Walk::new(*head.id(), self.repository).try_fold(
            HashMap::<crypto::Signature, NonEmpty<Oid>>::new(),
            |mut seen, commit| {
                let current = commit.map_err(error::Read::Commit)?;
                seen.entry(current.signature)
                    .and_modify(|value| value.push(current.oid))
                    .or_insert_with(|| NonEmpty::new(current.oid));
                Ok(seen)
            },
        )?;

        let parent = if seen
            .get(head.signature())
            .expect(SIGNATURES_COLLECTED)
            .len_nonzero()
            == ONE
        {
            // `head` has a verified, non-repeated signature, but does not
            // include the parent reference in the `/refs` blob. Maintains
            // backwards-compatibility.
            return Ok(head);
        } else {
            #[cfg(debug_assertions)]
            {
                if head.parent {
                    panic!("duplicate signature found even though parent ref did verify")
                }
            }

            // If the signature in head was seen twice, then
            // head must have a parent.
            *head.parent().expect("parent must exist")
        };

        // The second walk can start from the parent of head. We do not need to
        // verify head twice, and we already know that the parent exists.
        let mut last = None;
        for commit in iter::Walk::new(parent, self.repository) {
            let commit = commit
                .map_err(error::Read::Commit)?
                .verify(self.rid, self.verifier)
                .map_err(error::Read::Verify)?;

            let commits = seen.get(commit.signature()).expect(SIGNATURES_COLLECTED);

            if commits.len_nonzero() == ONE {
                return Ok(commit);
            } else {
                log::warn!("Duplicate sigrefs found in commits {commits:?}");
                last = Some(commit);
            }
        }

        // In the extreme case where all commits in the walk contain duplicate
        // signatures, return the oldest commit reached — the last one visited
        // by the walk, which follows the chain from newest to oldest.
        // `last` is always `Some` here because `parent` is guaranteed to exist
        // (head had a duplicate signature, so it must have a parent), meaning
        // the walk yields at least one commit.
        Ok(last.unwrap_or(head))
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
struct Commit {
    oid: Oid,
    parent: Option<Oid>,
    refs: Refs,
    signature: crypto::Signature,
    identity_root: Option<IdentityRoot>,
}

impl Commit {
    fn verify<V>(mut self, expected: RepoId, verifier: &V) -> Result<VerifiedCommit, error::Verify>
    where
        V: signature::Verifier<crypto::Signature>,
    {
        verifier
            .verify(&self.refs.canonical(), &self.signature)
            .map_err(error::Verify::Signature)?;

        if let Some(IdentityRoot {
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
                // Identity verification succeeds.
            }
        } else {
            let err = error::Verify::MissingIdentity(error::MissingIdentity {
                sigrefs_commit: self.oid,
                expected,
            });

            log::debug!("Reading sigrefs will error in the future: {err}");

            // TODO: Make this return the error
            // and enable test `test::commit_reader::missing_identity`.

            // Identity verification succeeds.
        }

        self.refs.remove_sigrefs();

        let parent = match (self.parent, self.refs.remove_parent()) {
            (None, None) => true,
            (Some(_), None) => false,
            (None, Some(actual)) => {
                return Err(error::Verify::DanglingParent {
                    sigrefs_commit: self.oid,
                    actual,
                })
            }
            (Some(expected), Some(actual)) if expected == actual => true,
            (Some(expected), Some(actual)) => {
                return Err(error::Verify::MismatchedParent {
                    sigrefs_commit: self.oid,
                    expected,
                    actual,
                })
            }
        };

        Ok(VerifiedCommit {
            commit: self,
            parent,
        })
    }
}

struct CommitReader<'a, R> {
    commit: Oid,
    repository: &'a R,
}

impl<'a, R> CommitReader<'a, R>
where
    R: object::Reader,
{
    fn new(commit: Oid, repository: &'a R) -> Self {
        Self { commit, repository }
    }

    fn read(self) -> Result<Commit, error::Commit> {
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
struct IdentityRoot {
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

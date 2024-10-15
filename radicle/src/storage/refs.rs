use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io;
use std::io::{BufRead, BufReader};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::str::FromStr;

use crypto::{PublicKey, Signature, Signer, SignerError, Unverified, Verified};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git;
use crate::git::ext as git_ext;
use crate::git::Oid;
use crate::profile::env;
use crate::storage;
use crate::storage::{ReadRepository, RemoteId, RepoId, WriteRepository};

pub use crate::git::refs::storage::*;

/// File in which the signed references are stored, in the `refs/rad/sigrefs` branch.
pub const REFS_BLOB_PATH: &str = "refs";
/// File in which the signature over the references is stored in the `refs/rad/sigrefs` branch.
pub const SIGNATURE_BLOB_PATH: &str = "signature";

#[derive(Debug)]
pub enum Updated {
    /// The computed [`Refs`] were stored as a new commit.
    Updated { oid: Oid },
    /// The stored [`Refs`] were the same as the computed ones, so no new commit
    /// was created.
    Unchanged { oid: Oid },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid signature: {0}")]
    InvalidSignature(#[from] crypto::Error),
    #[error("signer error: {0}")]
    Signer(#[from] SignerError),
    #[error("canonical refs: {0}")]
    Canonical(#[from] canonical::Error),
    #[error("invalid reference")]
    InvalidRef,
    #[error("missing identity root reference '{0}'")]
    MissingIdentityRoot(git::RefString),
    #[error("missing identity object '{0}'")]
    MissingIdentity(Oid),
    #[error("mismatched identity: local {local}, remote {remote}")]
    MismatchedIdentity { local: RepoId, remote: RepoId },
    #[error("invalid reference: {0}")]
    Ref(#[from] git::RefError),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    GitExt(#[from] git_ext::Error),
}

impl Error {
    /// Whether this error is caused by a reference not being found.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::GitExt(git::Error::NotFound(_)) => true,
            Self::GitExt(git::Error::Git(e)) if git::is_not_found_err(e) => true,
            Self::Git(e) if git::is_not_found_err(e) => true,
            _ => false,
        }
    }
}

// TODO(finto): we should turn `git::RefString` to `git::Qualified`,
// since all these refs SHOULD be `Qualified`.
/// The published state of a local repository.
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refs(BTreeMap<git::RefString, Oid>);

impl Refs {
    /// Verify the given signature on these refs, and return [`SignedRefs`] on success.
    pub fn verified<R: ReadRepository>(
        self,
        signer: PublicKey,
        signature: Signature,
        repo: &R,
    ) -> Result<SignedRefs<Verified>, Error> {
        SignedRefs::new(self, signer, signature).verified(repo)
    }

    /// Sign these refs with the given signer and return [`SignedRefs`].
    pub fn signed<G>(self, signer: &G) -> Result<SignedRefs<Unverified>, Error>
    where
        G: Signer,
    {
        let refs = self;
        let msg = refs.canonical();
        let signature = signer.try_sign(&msg)?;

        Ok(SignedRefs::new(refs, *signer.public_key(), signature))
    }

    /// Get a particular ref.
    pub fn get(&self, name: &git::Qualified) -> Option<Oid> {
        self.0.get(name.to_ref_string().as_refstr()).copied()
    }

    /// Get a particular head ref.
    pub fn head(&self, name: impl AsRef<git::RefStr>) -> Option<Oid> {
        let branch = git::refname!("refs/heads").join(name);
        self.0.get(&branch).copied()
    }

    /// Create refs from a canonical representation.
    pub fn from_canonical(bytes: &[u8]) -> Result<Self, canonical::Error> {
        let reader = BufReader::new(bytes);
        let mut refs = BTreeMap::new();

        for line in reader.lines() {
            let line = line?;
            let (oid, name) = line
                .split_once(' ')
                .ok_or(canonical::Error::InvalidFormat)?;

            let name = git::RefString::try_from(name)?;
            let oid = Oid::from_str(oid)?;

            if oid.is_zero() {
                continue;
            }
            refs.insert(name, oid);
        }
        Ok(Self(refs))
    }

    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = String::new();

        for (name, oid) in self.iter() {
            buf.push_str(&oid.to_string());
            buf.push(' ');
            buf.push_str(name);
            buf.push('\n');
        }
        buf.into_bytes()
    }
}

impl IntoIterator for Refs {
    type Item = (git::RefString, Oid);
    type IntoIter = std::collections::btree_map::IntoIter<git::RefString, Oid>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<Refs> for BTreeMap<git::RefString, Oid> {
    fn from(refs: Refs) -> Self {
        refs.0
    }
}

impl<V> From<SignedRefs<V>> for Refs {
    fn from(signed: SignedRefs<V>) -> Self {
        signed.refs
    }
}

impl From<BTreeMap<git::RefString, Oid>> for Refs {
    fn from(refs: BTreeMap<git::RefString, Oid>) -> Self {
        Self(refs)
    }
}

impl Deref for Refs {
    type Target = BTreeMap<git::RefString, Oid>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Refs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Combination of [`Refs`] and a [`Signature`]. The signature is a cryptographic
/// signature over the refs. This allows us to easily verify if a set of refs
/// came from a particular key.
///
/// The type parameter keeps track of whether the signature was [`Verified`] or
/// [`Unverified`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignedRefs<V> {
    /// The signed refs.
    pub refs: Refs,
    /// The signature of the signer over the refs.
    #[serde(skip)]
    pub signature: Signature,
    /// This is the remote under which these refs exist, and the public key of the signer.
    pub id: PublicKey,

    #[serde(skip)]
    _verified: PhantomData<V>,
}

impl SignedRefs<Unverified> {
    pub fn new(refs: Refs, author: PublicKey, signature: Signature) -> Self {
        Self {
            refs,
            signature,
            id: author,
            _verified: PhantomData,
        }
    }

    pub fn verified<R: ReadRepository>(self, repo: &R) -> Result<SignedRefs<Verified>, Error> {
        match self.verify(repo) {
            Ok(()) => Ok(SignedRefs {
                refs: self.refs,
                signature: self.signature,
                id: self.id,
                _verified: PhantomData,
            }),
            Err(e) => Err(e),
        }
    }

    pub fn verify<R: ReadRepository>(&self, repo: &R) -> Result<(), Error> {
        let canonical = self.refs.canonical();
        let local = repo.id();

        // Verify signature.
        if let Err(e) = self.id.verify(canonical, &self.signature) {
            return Err(e.into());
        }
        // If the identity root was signed, verify it points to the right place.
        if let Some(id_root) = self.refs.get(&IDENTITY_ROOT) {
            // Get the identity at the given oid.
            let Ok(doc) = repo.identity_doc_at(id_root) else {
                return Err(Error::MissingIdentity(id_root));
            };
            let remote = RepoId::from(doc.blob);

            // Make sure the signed identity points to the local repo identity.
            if remote != local {
                return Err(Error::MismatchedIdentity { local, remote });
            }
        } else {
            // TODO(cloudhead): Make this into a hard error (`Error::MissingIdentityRoot`) for
            // repos that have migrated to the new identity document schema.
            log::debug!(
                target: "storage",
                "Signed ref verification for {} in {local}: {} is not provided",
                self.id, *IDENTITY_ROOT
            );
        }
        Ok(())
    }
}

impl SignedRefs<Verified> {
    pub fn load<S>(remote: RemoteId, repo: &S) -> Result<Self, Error>
    where
        S: ReadRepository,
    {
        let oid = repo.reference_oid(&remote, &SIGREFS_BRANCH)?;

        SignedRefs::load_at(oid, remote, repo)
    }

    pub fn load_at<S>(oid: Oid, remote: RemoteId, repo: &S) -> Result<Self, Error>
    where
        S: storage::ReadRepository,
    {
        let refs = repo.blob_at(oid, Path::new(REFS_BLOB_PATH))?;
        let signature = repo.blob_at(oid, Path::new(SIGNATURE_BLOB_PATH))?;
        let signature: crypto::Signature = signature.content().try_into()?;
        let refs = Refs::from_canonical(refs.content())?;

        SignedRefs::new(refs, remote, signature).verified(repo)
    }

    /// Save the signed refs to disk.
    /// This creates a new commit on the signed refs branch, and updates the branch pointer.
    pub fn save<S: WriteRepository>(&self, repo: &S) -> Result<Updated, Error> {
        let sigref = &SIGREFS_BRANCH;
        let remote = &self.id;
        let raw = repo.raw();

        // N.b. if the signatures match then there are no updates
        let parent = match SignedRefsAt::load(*remote, repo)? {
            Some(SignedRefsAt { sigrefs, at }) if sigrefs.signature == self.signature => {
                return Ok(Updated::Unchanged { oid: at });
            }
            Some(SignedRefsAt { at, .. }) => Some(raw.find_commit(*at)?),
            None => None,
        };

        let tree = {
            let refs_blob_oid = raw.blob(&self.canonical())?;
            let sig_blob_oid = raw.blob(self.signature.as_ref())?;

            let mut builder = raw.treebuilder(None)?;
            builder.insert(REFS_BLOB_PATH, refs_blob_oid, 0o100_644)?;
            builder.insert(SIGNATURE_BLOB_PATH, sig_blob_oid, 0o100_644)?;

            let oid = builder.write()?;

            raw.find_tree(oid)
        }?;

        let sigref = sigref.with_namespace(remote.into());
        let author = if let Ok(s) = env::var(env::GIT_COMMITTER_DATE) {
            let Ok(timestamp) = s.trim().parse::<i64>() else {
                panic!(
                    "Invalid timestamp value {s:?} for `{}`",
                    env::GIT_COMMITTER_DATE
                );
            };
            let time = git2::Time::new(timestamp, 0);
            git2::Signature::new("radicle", remote.to_string().as_str(), &time)?
        } else {
            raw.signature()?
        };

        let commit = raw.commit(
            Some(&sigref),
            &author,
            &author,
            "Update signed refs\n",
            &tree,
            &parent.iter().collect::<Vec<&git2::Commit>>(),
        );

        match commit {
            Ok(oid) => Ok(Updated::Updated { oid: oid.into() }),
            Err(e) => match (e.class(), e.code()) {
                (git2::ErrorClass::Object, git2::ErrorCode::Modified) => {
                    log::warn!("Concurrent modification of refs: {:?}", e);

                    Err(Error::Git(e))
                }
                _ => Err(e.into()),
            },
        }
    }

    pub fn unverified(self) -> SignedRefs<Unverified> {
        SignedRefs {
            refs: self.refs,
            signature: self.signature,
            id: self.id,
            _verified: PhantomData,
        }
    }
}

impl<V> Deref for SignedRefs<V> {
    type Target = Refs;

    fn deref(&self) -> &Self::Target {
        &self.refs
    }
}

/// The content-addressable information required to load a remote's
/// `rad/sigrefs`.
///
/// This can be used to [`RefsAt::load`] a [`SignedRefsAt`].
///
/// It can also be used for communicating announcements of updates
/// references to other nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefsAt {
    /// The remote namespace of the `rad/sigrefs`.
    pub remote: RemoteId,
    /// The commit SHA that `rad/sigrefs` points to.
    pub at: Oid,
}

impl RefsAt {
    pub fn new<S: ReadRepository>(repo: &S, remote: RemoteId) -> Result<Self, git::raw::Error> {
        let at = repo.reference_oid(&remote, &storage::refs::SIGREFS_BRANCH)?;
        Ok(RefsAt { remote, at })
    }

    pub fn load<S: ReadRepository>(&self, repo: &S) -> Result<SignedRefsAt, Error> {
        SignedRefsAt::load_at(self.at, self.remote, repo)
    }

    pub fn path(&self) -> &git::Qualified {
        &SIGREFS_BRANCH
    }
}

/// Verified [`SignedRefs`] that keeps track of their content address
/// [`Oid`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRefsAt {
    pub sigrefs: SignedRefs<Verified>,
    pub at: Oid,
}

impl SignedRefsAt {
    /// Load the [`SignedRefs`] found under `remote`'s [`SIGREFS_BRANCH`].
    ///
    /// This will return `None` if the branch was not found, all other
    /// errors are returned.
    pub fn load<S>(remote: RemoteId, repo: &S) -> Result<Option<Self>, Error>
    where
        S: ReadRepository,
    {
        let at = match RefsAt::new(repo, remote) {
            Ok(RefsAt { at, .. }) => at,
            Err(e) if git::is_not_found_err(&e) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Self::load_at(at, remote, repo).map(Some)
    }

    pub fn load_at<S>(at: Oid, remote: RemoteId, repo: &S) -> Result<Self, Error>
    where
        S: storage::ReadRepository,
    {
        Ok(Self {
            sigrefs: SignedRefs::load_at(at, remote, repo)?,
            at,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&git::RefString, &Oid)> {
        self.sigrefs.refs.iter()
    }
}

impl Deref for SignedRefsAt {
    type Target = SignedRefs<Verified>;

    fn deref(&self) -> &Self::Target {
        &self.sigrefs
    }
}

pub mod canonical {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error(transparent)]
        InvalidRef(#[from] git::fmt::Error),
        #[error("invalid canonical format")]
        InvalidFormat,
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crypto::test::signer::MockSigner;
    use qcheck_macros::quickcheck;
    use storage::{git::transport, RemoteRepository, SignRepository, WriteStorage};

    use super::*;
    use crate::assert_matches;
    use crate::{cob::identity::Identity, rad, test::fixtures, Storage};

    #[quickcheck]
    fn prop_canonical_roundtrip(refs: Refs) {
        let encoded = refs.canonical();
        let decoded = Refs::from_canonical(&encoded).unwrap();

        assert_eq!(refs, decoded);
    }

    #[test]
    // Test that a user's signed refs are tied to a specific RID, and they can't simply be
    // used in a different repository.
    //
    // We create two repos, `paris` and `london`, and we copy over Bob's signed refs from `paris`
    // to `london`. We expect that this does not cause the canonical head of the `london` repo
    // to change, despite Bob being a delegate of both repos, because the refs were signed for the
    // `paris` repo. We also don't expected the signed refs to validate without error.
    fn test_rid_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let alice = MockSigner::default();
        let bob = MockSigner::default();
        let storage = &Storage::open(tmp.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        // Alice creates "paris" repo.
        let (paris_repo, paris_head) = fixtures::repository(tmp.path().join("paris"));
        let (paris_rid, paris_doc, _) = rad::init(
            &paris_repo,
            "paris".try_into().unwrap(),
            "Paris repository",
            git::refname!("master"),
            Default::default(),
            &alice,
            storage,
        )
        .unwrap();

        // Alice creates "london" repo.
        let (london_repo, _london_head) = fixtures::repository(tmp.path().join("london"));
        let (london_rid, london_doc, _) = rad::init(
            &london_repo,
            "london".try_into().unwrap(),
            "London repository",
            git::refname!("master"),
            Default::default(),
            &alice,
            storage,
        )
        .unwrap();

        assert_ne!(london_rid, paris_rid);

        log::debug!(target: "test", "London RID: {london_rid}");
        log::debug!(target: "test", "Paris RID: {paris_rid}");

        let paris = storage.repository_mut(paris_rid).unwrap();
        let london = storage.repository_mut(london_rid).unwrap();

        // Bob is added to both repos as a delegate, by Alice.
        {
            let paris_doc = paris_doc
                .with_edits(|doc| {
                    doc.delegates.push(bob.public_key().into());
                })
                .unwrap();
            let london_doc = london_doc
                .with_edits(|doc| {
                    doc.delegates.push(bob.public_key().into());
                })
                .unwrap();

            let mut paris_ident = Identity::load_mut(&paris).unwrap();
            let mut london_ident = Identity::load_mut(&london).unwrap();

            paris_ident
                .update("Add Bob", "", &paris_doc, &alice)
                .unwrap();
            london_ident
                .update("Add Bob", "", &london_doc, &alice)
                .unwrap();
        }

        // Now Bob checks out a copy of the `paris` repository and pushes a commit to the
        // default branch (master). We store the OID of that commti in `bob_head`, as this
        // is the commit we will try to get the `london` repo to point to.
        let (bob_paris_sigrefs, bob_head) = {
            let bob_working = rad::checkout(
                paris.id,
                bob.public_key(),
                tmp.path().join("working"),
                &storage,
            )
            .unwrap();

            let paris_head = bob_working.find_commit(paris_head).unwrap();
            let bob_sig = git2::Signature::now("bob", "bob@example.com").unwrap();
            let bob_head = git::empty_commit(
                &bob_working,
                &paris_head,
                git::refname!("refs/heads/master").as_refstr(),
                "Bob's commit",
                &bob_sig,
            )
            .unwrap();

            let mut bob_master_ref = bob_working.find_reference("refs/heads/master").unwrap();
            bob_master_ref.set_target(bob_head.id(), "").unwrap();
            bob_working
                .find_remote("rad")
                .unwrap()
                .push(&["refs/heads/master"], None)
                .unwrap();
            let sigrefs = paris.sign_refs(&bob).unwrap();

            assert_eq!(
                sigrefs
                    .get(&git_ext::ref_format::qualified!("refs/heads/master"))
                    .unwrap(),
                bob_head.id().into()
            );
            (sigrefs, bob_head.id())
        };

        {
            // Sanity check: make sure the default branches don't already match between Alice and Bob.
            let alice_paris_sigrefs = SignedRefsAt::load(*alice.public_key(), &paris)
                .unwrap()
                .unwrap();
            assert_ne!(
                alice_paris_sigrefs
                    .get(&git_ext::ref_format::qualified!("refs/heads/master"))
                    .unwrap(),
                bob_paris_sigrefs
                    .get(&git_ext::ref_format::qualified!("refs/heads/master"))
                    .unwrap()
            );
        }

        {
            // For the graft to work, we also have to copy over the objects that Bob created in
            // `paris`, so that the grafted signed refs point to valid objects.
            let paris_odb = paris.raw().odb().unwrap();
            let london_odb = london.raw().odb().unwrap();

            paris_odb
                .foreach(|oid| {
                    let obj = paris_odb.read(*oid).unwrap();
                    london_odb.write(obj.kind(), obj.data()).unwrap();

                    true
                })
                .unwrap();
        }
        // Now we're going to "graft" Bob's signed refs from `paris` to `london`.
        // We save Bob's `paris` signed refs in the `london` repo, performing the graft, and update
        // Bob's `master` branch reference to point to his commit, created in the `paris` repo. This
        // only modifies his own namespace. Note that anyone (eg. Eve) could create a reference
        // under her copy of Bob's namespace, and this would only be rejected during signed ref
        // validation.
        let result = bob_paris_sigrefs.save(&london).unwrap();
        assert_matches!(result, Updated::Updated { .. });

        london
            .raw()
            .reference(
                git::refs::storage::branch_of(bob.public_key(), &git::refname!("master")).as_str(),
                bob_head,
                false,
                "",
            )
            .unwrap();

        // Due to the verification, we get a validation error when trying to load Bob's remote.
        // The graft is not allowed.
        assert_matches!(
            london.remote(bob.public_key()),
            Err(Error::MismatchedIdentity {
                local,
                remote,
            })
            if local == london_rid && remote == paris_rid
        );
    }
}

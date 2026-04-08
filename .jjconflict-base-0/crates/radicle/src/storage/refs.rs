pub mod sigrefs;

#[cfg(any(test, feature = "test"))]
pub mod arbitrary;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io;
use std::io::{BufRead, BufReader};
use std::ops::Deref;
use std::str::FromStr;

use crypto::signature;
use crypto::{PublicKey, Signature};
use radicle_core::NodeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git;
use crate::git::Oid;
use crate::git::raw::ErrorExt as _;
use crate::storage;
use crate::storage::RemoteId;
use crate::storage::refs::sigrefs::read::Tip;

pub use crate::git::refs::storage::*;

use super::HasRepoId;

/// File in which the signed references are stored, in the `refs/rad/sigrefs` branch.
pub const REFS_BLOB_PATH: &str = "refs";
/// File in which the signature over the references is stored in the `refs/rad/sigrefs` branch.
pub const SIGNATURE_BLOB_PATH: &str = "signature";

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid reference")]
    InvalidRef,
    #[error("invalid reference: {0}")]
    Ref(#[from] git::RefError),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    Read(#[from] sigrefs::read::error::Read),
    #[error(transparent)]
    Write(#[from] sigrefs::write::error::Write),
}

impl Error {
    /// Whether this error is caused by a reference not being found.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Git(e) => e.is_not_found(),
            Self::Read(sigrefs::read::error::Read::MissingSigrefs { .. }) => true,
            _ => false,
        }
    }
}

// TODO(finto): we should turn `git::fmt::RefString` to `git::fmt::Qualified`,
// since all these refs SHOULD be `Qualified`.
/// The published state of a local repository.
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refs(BTreeMap<git::fmt::RefString, Oid>);

impl Refs {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Save the signed refs to disk.
    /// This creates a new commit on the signed refs branch, and updates the branch pointer.
    pub fn save<R, S>(
        self,
        namespace: NodeId,
        committer: sigrefs::git::Committer,
        repo: &R,
        signer: &S,
    ) -> Result<SignedRefs, Error>
    where
        R: sigrefs::git::object::Reader + sigrefs::git::object::Writer,
        R: sigrefs::git::reference::Reader + sigrefs::git::reference::Writer,
        R: HasRepoId,
        S: signature::Signer<crypto::Signature>,
        S: signature::Verifier<crypto::Signature>,
    {
        self.save_with(namespace, committer, repo, signer, false)
    }

    /// Save the signed refs to disk, even if the refs are unchanged.
    pub fn force_save<R, S>(
        self,
        namespace: NodeId,
        committer: sigrefs::git::Committer,
        repo: &R,
        signer: &S,
    ) -> Result<SignedRefs, Error>
    where
        R: sigrefs::git::object::Reader + sigrefs::git::object::Writer,
        R: sigrefs::git::reference::Reader + sigrefs::git::reference::Writer,
        R: HasRepoId,
        S: signature::Signer<crypto::Signature>,
        S: signature::Verifier<crypto::Signature>,
    {
        self.save_with(namespace, committer, repo, signer, true)
    }

    fn save_with<R, S>(
        self,
        namespace: NodeId,
        committer: sigrefs::git::Committer,
        repo: &R,
        signer: &S,
        force: bool,
    ) -> Result<SignedRefs, Error>
    where
        R: sigrefs::git::object::Reader + sigrefs::git::object::Writer,
        R: sigrefs::git::reference::Reader + sigrefs::git::reference::Writer,
        R: HasRepoId,
        S: signature::Signer<crypto::Signature>,
        S: signature::Verifier<crypto::Signature>,
    {
        let msg = "Update signed refs\n";
        let reflog = format!("Save {} signed references", self.len());
        let writer =
            sigrefs::write::SignedRefsWriter::new(self, repo.rid(), namespace, repo, signer);
        let update = if force {
            writer.force_write(committer, msg.to_string(), reflog)?
        } else {
            writer.write(committer, msg.to_string(), reflog)?
        };
        match update {
            sigrefs::write::Update::Changed { entry, level } => {
                Ok(entry.into_sigrefs_at(namespace, level))
            }
            sigrefs::write::Update::Unchanged { verified } => {
                Ok(verified.into_sigrefs_at(namespace))
            }
        }
    }

    /// Get a particular ref.
    pub fn get(&self, name: &git::fmt::Qualified) -> Option<Oid> {
        self.0.get(name.to_ref_string().as_refstr()).copied()
    }

    /// Get a particular head ref.
    pub fn head(&self, name: impl AsRef<git::fmt::RefStr>) -> Option<Oid> {
        let branch = git::fmt::refname!("refs/heads").join(name);
        self.0.get(&branch).copied()
    }

    /// Create refs from a canonical representation.
    fn from_canonical(bytes: &[u8]) -> Result<Self, canonical::Error> {
        let reader = BufReader::new(bytes);
        let mut refs = BTreeMap::new();

        for line in reader.lines() {
            let line = line?;
            let (oid, name) = line
                .split_once(' ')
                .ok_or(canonical::Error::InvalidFormat)?;

            let name = git::fmt::RefString::try_from(name)?;
            let oid = Oid::from_str(oid).map_err(|_| canonical::Error::InvalidFormat)?;

            if oid.is_zero() || name.as_refstr() == SIGREFS_BRANCH.as_ref() {
                continue;
            }

            refs.insert(name, oid);
        }
        Ok(Self(refs))
    }

    fn canonical(&self) -> Vec<u8> {
        let mut buf = String::new();

        for (name, oid) in self.0.iter() {
            debug_assert_ne!(oid, &Oid::sha1_zero());
            debug_assert_ne!(name, &SIGREFS_BRANCH.to_ref_string());

            buf.push_str(&oid.to_string());
            buf.push(' ');
            buf.push_str(name);
            buf.push('\n');
        }

        buf.into_bytes()
    }

    pub fn insert(&mut self, refname: git::fmt::RefString, target: Oid) -> Option<Oid> {
        if target.is_zero() {
            self.0.remove(&refname)
        } else {
            self.0.insert(refname, target)
        }
    }

    pub(crate) fn keys<'a>(
        &'a self,
    ) -> std::collections::btree_map::Keys<'a, git::fmt::RefString, Oid> {
        self.0.keys()
    }

    #[cfg(any(test, feature = "test"))]
    pub(crate) fn values<'a>(
        &'a self,
    ) -> std::collections::btree_map::Values<'a, git::fmt::RefString, Oid> {
        self.0.values()
    }

    pub fn iter<'a>(&'a self) -> std::collections::btree_map::Iter<'a, git::fmt::RefString, Oid> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn remove_sigrefs(&mut self) -> Option<Oid> {
        self.0.remove(&SIGREFS_BRANCH.to_ref_string())
    }

    /// Add a reference with name [`crate::git::refs::storage::SIGREFS_PARENT`]
    /// and given target OID to this set of refs.
    #[inline]
    fn add_parent(&mut self, commit: Oid) -> Option<Oid> {
        self.0.insert(SIGREFS_PARENT.to_ref_string(), commit)
    }

    /// Removes reference with name [`crate::git::refs::storage::SIGREFS_PARENT`]
    /// from this set of refs, if it exists.
    /// Absence of a reference with such name is ignored.
    #[inline]
    fn remove_parent(&mut self) -> Option<Oid> {
        self.0.remove(&SIGREFS_PARENT.to_ref_string())
    }
}

impl IntoIterator for Refs {
    type Item = (git::fmt::RefString, Oid);
    type IntoIter = std::collections::btree_map::IntoIter<git::fmt::RefString, Oid>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<Refs> for BTreeMap<git::fmt::RefString, Oid> {
    fn from(refs: Refs) -> Self {
        refs.0
    }
}

impl<I> From<I> for Refs
where
    I: Iterator<Item = (git::fmt::RefString, Oid)>,
{
    fn from(value: I) -> Self {
        let mut refs = Self::new();
        for (refname, target) in value {
            refs.insert(refname, target);
        }
        refs
    }
}

/// The Signed References feature has evolved over time.
/// This enum captures the corresponding "feature level".
///
/// Feature levels are monotonic, in the sense that a greater feature level
/// encompasses all the features of smaller ones.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum FeatureLevel {
    /// The lowest feature level, with least security. It is vulnerable to
    /// graft attacks and replay attacks.
    #[default]
    None,

    #[cfg_attr(
        feature = "schemars",
        schemars(description = "\
        An intermediate feature level, which protects against graft attacks \
        but is vulnerable to replay attacks. \
        Introduced in Radicle 1.1.0, in commit \
        `989edacd564fa658358f5ccfd08c243c5ebd8cda`.\
    ")
    )]
    /// Requires [`IDENTITY_ROOT`].
    Root,

    #[cfg_attr(
        feature = "schemars",
        schemars(description = "\
        The highest feature level known, which protects against graft attacks \
        and replay attacks. \
        Introduced in Radicle 1.7.0, in commit \
        `d3bc868e84c334f113806df1737f52cc57c5453d`.\
    ")
    )]
    /// Requires [`SIGREFS_PARENT`].
    Parent,
}

impl FeatureLevel {
    pub const LATEST: Self = FeatureLevel::Parent;
}

impl std::fmt::Display for FeatureLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match &self {
            Self::None => "none",
            Self::Root => "root",
            Self::Parent => "parent",
        };
        f.write_str(s)
    }
}

/// The content-addressable information required to load a remote's
/// `rad/sigrefs`.
///
/// Use [`RefsAt::remote`] and [`RefsAt::at`] with [`SignedRefs::load_at`] to
/// attempt loading the expected signed references.
///
/// `RefsAt` can also be used for communicating announcements of updates
/// references to other nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RefsAt {
    /// The remote namespace of the `rad/sigrefs`.
    pub remote: RemoteId,
    /// The commit SHA that `rad/sigrefs` points to.
    pub at: Oid,
}

impl RefsAt {
    pub fn new<R>(repo: &R, remote: RemoteId) -> Result<Self, sigrefs::read::error::Read>
    where
        R: sigrefs::git::reference::Reader,
    {
        let at = repo
            .find_reference(
                &storage::refs::SIGREFS_BRANCH.with_namespace(git::fmt::Component::from(&remote)),
            )
            .map_err(sigrefs::read::error::Read::FindReference)?
            .ok_or_else(|| sigrefs::read::error::Read::MissingSigrefs { namespace: remote })?;
        Ok(RefsAt { remote, at })
    }

    pub fn path(&self) -> &git::fmt::Qualified<'_> {
        &SIGREFS_BRANCH
    }
}

impl std::fmt::Display for RefsAt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} @ {}", self.remote, self.at)
    }
}

/// Verified [`SignedRefs`] that keeps track of their content address
/// [`Oid`].
///
/// The signature is a cryptographic signature over the refs.
/// This allows us to easily verify if a set of refs
/// came from a particular key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignedRefs {
    /// The signed refs.
    refs: Refs,
    /// The signature of the signer over the refs.
    #[serde(skip)]
    signature: Signature,
    /// This is the remote under which these refs exist, and the public key of the signer.
    id: PublicKey,

    #[serde(skip)]
    level: FeatureLevel,

    /// The [`Oid`] of the parent commit of the commit in which.
    #[serde(skip)]
    parent: Option<Oid>,

    pub at: Oid,
}

impl SignedRefs {
    /// Returns the [`NodeId`] of the [`SignedRefs`].
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the [`Refs`] of the [`SignedRefs`].
    pub fn refs(&self) -> &Refs {
        &self.refs
    }

    /// Returns the [`FeatureLevel`] computed for the signed references.
    pub fn feature_level(&self) -> FeatureLevel {
        self.level
    }

    /// The [`Oid`] of the parent commit, or [`None`] if these signed references
    /// were found at a root commit.
    pub fn parent(&self) -> Option<&Oid> {
        self.parent.as_ref()
    }

    /// Load the [`SignedRefs`] found under `remote`'s [`SIGREFS_BRANCH`].
    ///
    /// This will return `None` if the branch was not found, all other
    /// errors are returned.
    pub fn load<R>(remote: RemoteId, repo: &R) -> Result<Option<Self>, sigrefs::read::error::Read>
    where
        R: HasRepoId,
        R: sigrefs::git::object::Reader + sigrefs::git::reference::Reader,
    {
        Self::load_internal(remote, repo, sigrefs::read::Tip::Reference(remote))
    }

    pub fn load_at<R>(
        oid: Oid,
        remote: RemoteId,
        repo: &R,
    ) -> Result<Option<Self>, sigrefs::read::error::Read>
    where
        R: HasRepoId,
        R: sigrefs::git::object::Reader + sigrefs::git::reference::Reader,
    {
        Self::load_internal(remote, repo, sigrefs::read::Tip::Commit(oid))
    }

    fn load_internal<R>(
        remote: RemoteId,
        repo: &R,
        tip: Tip,
    ) -> Result<Option<Self>, sigrefs::read::error::Read>
    where
        R: HasRepoId,
        R: sigrefs::git::object::Reader + sigrefs::git::reference::Reader,
    {
        let root = repo.rid();
        match sigrefs::SignedRefsReader::new(root, tip, repo, &remote).read() {
            Ok(latest) => Ok(Some(latest.into_sigrefs_at(remote))),
            Err(sigrefs::read::error::Read::MissingSigrefs { namespace }) => {
                debug_assert_eq!(namespace, remote);
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&git::fmt::RefString, &Oid)> {
        self.refs.iter()
    }
}

impl Deref for SignedRefs {
    type Target = Refs;

    fn deref(&self) -> &Self::Target {
        &self.refs
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
        Git(#[from] git::raw::Error),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use qcheck_macros::quickcheck;
    use storage::{RemoteRepository, SignRepository, WriteStorage, git::transport};

    use super::*;
    use crate::assert_matches;

    use crate::node::device::Device;
    use crate::storage::WriteRepository as _;
    use crate::{Storage, cob::Title, cob::identity::Identity, rad, test::fixtures};

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
        let alice = Device::mock();
        let bob = Device::mock();
        let storage = &Storage::open(tmp.path().join("storage"), fixtures::user()).unwrap();

        transport::local::register(storage.clone());

        // Alice creates "paris" repo.
        let (paris_repo, paris_head) = fixtures::repository(tmp.path().join("paris"));
        let (paris_rid, paris_doc, _) = rad::init(
            &paris_repo,
            "paris".try_into().unwrap(),
            "Paris repository",
            git::fmt::refname!("master"),
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
            git::fmt::refname!("master"),
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

            let mut paris_ident = Identity::load_mut(&paris, &alice).unwrap();
            let mut london_ident = Identity::load_mut(&london, &alice).unwrap();

            paris_ident
                .update(Title::new("Add Bob").unwrap(), "", &paris_doc)
                .unwrap();
            london_ident
                .update(Title::new("Add Bob").unwrap(), "", &london_doc)
                .unwrap();
        }

        // Now Bob checks out a copy of the `paris` repository and pushes a commit to the
        // default branch (master). We store the OID of that commit in `bob_head`, as this
        // is the commit we will try to get the `london` repo to point to.
        let (bob_paris_sigrefs, bob_head) = {
            let bob_working = rad::checkout(
                paris.id,
                bob.public_key(),
                tmp.path().join("working"),
                &storage,
                false,
            )
            .unwrap();

            let paris_head = bob_working.find_commit(paris_head).unwrap();
            let bob_sig = git::raw::Signature::now("bob", "bob@example.com").unwrap();
            let bob_head = git::empty_commit(
                &bob_working,
                &paris_head,
                git::fmt::refname!("refs/heads/master").as_refstr(),
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
                    .get(&crate::git::fmt::qualified!("refs/heads/master"))
                    .unwrap(),
                bob_head.id()
            );
            (sigrefs, bob_head.id())
        };

        {
            // Sanity check: make sure the default branches don't already match between Alice and Bob.
            let alice_paris_sigrefs = SignedRefs::load(*alice.public_key(), &paris)
                .unwrap()
                .unwrap();
            assert_ne!(
                alice_paris_sigrefs
                    .get(&crate::git::fmt::qualified!("refs/heads/master"))
                    .unwrap(),
                bob_paris_sigrefs
                    .get(&crate::git::fmt::qualified!("refs/heads/master"))
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
        {
            let name = &SIGREFS_BRANCH.with_namespace(git::fmt::Component::from(bob.node_id()));
            let id = paris.backend.refname_to_id(name.as_str()).unwrap();
            london
                .backend
                .reference(name.as_str(), id, true, "Graft attack")
                .unwrap();
        }

        london
            .raw()
            .reference(
                git::refs::storage::branch_of(bob.public_key(), &git::fmt::refname!("master"))
                    .as_str(),
                bob_head,
                false,
                "",
            )
            .unwrap();

        // Due to the verification, we get a validation error when trying to load Bob's remote.
        // The graft is not allowed.
        assert_matches!(
            london.remote(bob.public_key()),
            Err(Error::Read(sigrefs::read::error::Read::Verify(sigrefs::read::error::Verify::MismatchedIdentity {
                expected,
                found,
                sigrefs_commit: _,
                identity_commit: _,
            })))
            if expected == london_rid && found == paris_rid
        );
    }
}

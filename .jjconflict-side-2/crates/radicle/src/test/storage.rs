use std::collections::HashMap;
use std::convert::Infallible;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crypto::PublicKey;

pub use crate::git;
use crate::git::fmt;

use crate::identity::doc::{Doc, DocAt, DocError, RawDoc, RepoId};
use crate::node::NodeId;

pub use crate::storage::*;

use super::{arbitrary, fixtures};

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub path: PathBuf,
    pub info: crate::git::UserInfo,

    /// All refs keyed by RID.
    /// Each value is a map of refs keyed by node Id (public key).
    pub repos: HashMap<RepoId, MockRepository>,
}

impl MockStorage {
    pub fn new(inventory: Vec<(RepoId, DocAt)>) -> Self {
        Self {
            path: PathBuf::default(),
            info: fixtures::user(),
            repos: inventory
                .into_iter()
                .map(|(id, doc)| {
                    (
                        id,
                        MockRepository {
                            id,
                            doc,
                            remotes: HashMap::new(),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn repo_mut(&mut self, rid: &RepoId) -> &mut MockRepository {
        self.repos
            .get_mut(rid)
            .expect("MockStorage::repo_mut: repository does not exist")
    }

    pub fn map(mut self, f: impl Fn(&mut RawDoc)) -> Self {
        for repo in self.repos.values_mut() {
            repo.doc.doc = repo.doc.doc.clone().with_edits(|doc| f(doc)).unwrap();
        }
        self
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }
}

impl ReadStorage for MockStorage {
    type Repository = MockRepository;

    fn info(&self) -> &crate::git::UserInfo {
        &self.info
    }

    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn path_of(&self, rid: &RepoId) -> PathBuf {
        self.path().join(rid.canonical())
    }

    fn contains(&self, rid: &RepoId) -> Result<bool, RepositoryError> {
        Ok(self.repos.contains_key(rid))
    }

    fn repository(&self, rid: RepoId) -> Result<Self::Repository, RepositoryError> {
        self.repos
            .get(&rid)
            .ok_or_else(|| {
                RepositoryError::from(Error::Io(io::Error::from(io::ErrorKind::NotFound)))
            })
            .cloned()
    }

    fn repositories(&self) -> Result<Vec<RepositoryInfo>, Error> {
        Ok(self
            .repos
            .iter()
            .map(|(rid, r)| RepositoryInfo {
                rid: *rid,
                head: r.head().unwrap().1,
                doc: r.doc.clone().into(),
                refs: SignedRefsInfo::None,
                synced_at: None,
            })
            .collect())
    }
}

impl WriteStorage for MockStorage {
    type RepositoryMut = MockRepository;

    fn repository_mut(&self, rid: RepoId) -> Result<Self::RepositoryMut, RepositoryError> {
        self.repos
            .get(&rid)
            .ok_or(RepositoryError::from(Error::Io(io::Error::from(
                io::ErrorKind::NotFound,
            ))))
            .cloned()
    }

    fn create(&self, _rid: RepoId) -> Result<Self::RepositoryMut, Error> {
        todo!()
    }

    fn clean(&self, _rid: RepoId) -> Result<Vec<RemoteId>, RepositoryError> {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct MockRepository {
    pub id: RepoId,
    pub doc: DocAt,
    pub remotes: HashMap<NodeId, refs::SignedRefs>,
}

impl MockRepository {
    pub fn new(id: RepoId, doc: Doc) -> Self {
        let (blob, _) = doc.encode().unwrap();

        Self {
            id,
            doc: DocAt {
                commit: Oid::from_str("ffffffffffffffffffffffffffffffffffffffff").unwrap(),
                blob,
                doc,
            },
            remotes: HashMap::default(),
        }
    }
}

impl self::refs::sigrefs::git::reference::Reader for MockRepository {
    fn find_reference(
        &self,
        reference: &git::fmt::Namespaced,
    ) -> Result<Option<Oid>, refs::sigrefs::git::reference::error::FindReference> {
        use refs::sigrefs::git::reference::error::FindReference;
        let ns = reference.namespace();

        let remote: PublicKey = ns.as_str().parse().map_err(FindReference::other)?;
        let reference = reference.strip_namespace();

        match self.remotes.get(&remote) {
            None => Ok(None),
            Some(refs) => {
                if reference == *refs::SIGREFS_BRANCH {
                    Ok(Some(refs.at))
                } else {
                    Ok(refs.get(&reference))
                }
            }
        }
    }
}

impl RemoteRepository for MockRepository {
    fn remote(&self, id: &RemoteId) -> Result<Remote, refs::Error> {
        self.remotes
            .get(id)
            .map(|refs| Remote { refs: refs.clone() })
            .ok_or(refs::Error::InvalidRef)
    }

    fn remotes(&self) -> Result<Remotes, refs::Error> {
        Ok(self
            .remotes
            .iter()
            .map(|(id, refs)| (*id, Remote { refs: refs.clone() }))
            .collect())
    }

    fn remote_refs_at(&self) -> Result<Vec<refs::RefsAt>, refs::Error> {
        Ok(self
            .remotes
            .values()
            .map(|s| refs::RefsAt {
                remote: s.id(),
                at: s.at,
            })
            .collect())
    }
}

impl ValidateRepository for MockRepository {
    fn validate_remote(&self, _remote: &Remote) -> Result<Validations, Error> {
        Ok(Validations::default())
    }
}

impl ReadRepository for MockRepository {
    fn id(&self) -> RepoId {
        self.id
    }

    fn is_empty(&self) -> Result<bool, git::raw::Error> {
        Ok(self.remotes.is_empty())
    }

    fn head(&self) -> Result<(fmt::Qualified<'_>, Oid), RepositoryError> {
        Ok((fmt::qualified!("refs/heads/master"), arbitrary::oid()))
    }

    fn canonical_head(&self) -> Result<(fmt::Qualified<'_>, Oid), RepositoryError> {
        todo!()
    }

    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn commit(&self, oid: Oid) -> Result<git::raw::Commit<'_>, git::raw::Error> {
        Err(git::raw::Error::new(
            git::raw::ErrorCode::NotFound,
            git::raw::ErrorClass::None,
            format!("commit {oid} not found"),
        ))
    }

    fn revwalk(&self, _head: Oid) -> Result<git::raw::Revwalk<'_>, git::raw::Error> {
        todo!()
    }

    fn contains(&self, oid: Oid) -> Result<bool, git::raw::Error> {
        Ok(self
            .remotes
            .values()
            .any(|sigrefs| sigrefs.at == oid || sigrefs.values().any(|oid_| *oid_ == oid)))
    }

    fn is_ancestor_of(&self, _ancestor: Oid, _head: Oid) -> Result<bool, crate::git::raw::Error> {
        Ok(true)
    }

    fn blob(&self, _oid: Oid) -> Result<git::raw::Blob<'_>, git::raw::Error> {
        todo!()
    }

    fn blob_at<P: AsRef<std::path::Path>>(
        &self,
        _oid: Oid,
        _path: P,
    ) -> Result<git::raw::Blob<'_>, git::raw::Error> {
        todo!()
    }

    fn reference(
        &self,
        _remote: &RemoteId,
        _reference: &git::fmt::Qualified,
    ) -> Result<git::raw::Reference<'_>, git::raw::Error> {
        todo!()
    }

    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &crate::git::fmt::Qualified,
    ) -> Result<Oid, crate::git::raw::Error> {
        let not_found = || {
            crate::git::raw::Error::new(
                crate::git::raw::ErrorCode::NotFound,
                crate::git::raw::ErrorClass::Reference,
                format!("could not find {reference} for {remote}"),
            )
        };

        let refs = self.remotes.get(remote).ok_or_else(not_found)?;
        if reference == &*refs::SIGREFS_BRANCH {
            Ok(refs.at)
        } else {
            refs.get(reference).ok_or_else(not_found)
        }
    }

    fn references_of(&self, _remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }

    fn references_glob(
        &self,
        _pattern: &crate::git::fmt::refspec::PatternStr,
    ) -> Result<Vec<(fmt::Qualified<'_>, Oid)>, crate::git::raw::Error> {
        todo!()
    }

    fn identity_doc(&self) -> Result<crate::identity::DocAt, RepositoryError> {
        Ok(self.doc.clone())
    }

    fn identity_doc_at(&self, _head: Oid) -> Result<crate::identity::DocAt, DocError> {
        Ok(self.doc.clone())
    }

    fn identity_head(&self) -> Result<Oid, RepositoryError> {
        self.canonical_identity_head()
    }

    fn identity_head_of(&self, _remote: &RemoteId) -> Result<Oid, crate::git::raw::Error> {
        Ok(self.doc.commit)
    }

    fn identity_root(&self) -> Result<Oid, RepositoryError> {
        Ok(self.doc.commit)
    }

    fn identity_root_of(&self, _remote: &RemoteId) -> Result<Oid, RepositoryError> {
        Ok(self.doc.commit)
    }

    fn canonical_identity_head(&self) -> Result<Oid, RepositoryError> {
        Ok(self.doc.commit)
    }

    fn merge_base(&self, _left: &Oid, _right: &Oid) -> Result<Oid, crate::git::raw::Error> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn raw(&self) -> &git::raw::Repository {
        todo!()
    }

    fn set_head(&self) -> Result<SetHead, RepositoryError> {
        todo!()
    }

    fn set_identity_head_to(&self, _commit: Oid) -> Result<(), RepositoryError> {
        todo!()
    }

    fn set_remote_identity_root_to(
        &self,
        _remote: &RemoteId,
        _root: Oid,
    ) -> Result<(), RepositoryError> {
        todo!()
    }

    fn set_user(&self, _info: &crate::git::UserInfo) -> Result<(), Error> {
        todo!()
    }
}

impl SignRepository for MockRepository {
    fn sign_refs<Signer>(
        &self,
        _signer: &Signer,
    ) -> Result<crate::storage::refs::SignedRefs, RepositoryError>
    where
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
    {
        todo!()
    }

    fn force_sign_refs<Signer>(&self, _signer: &Signer) -> Result<refs::SignedRefs, RepositoryError>
    where
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
        Signer: crypto::signature::Verifier<crypto::Signature>,
    {
        todo!()
    }
}

impl radicle_cob::Store for MockRepository {}

impl radicle_cob::object::Storage for MockRepository {
    type ObjectsError = Infallible;
    type TypesError = Infallible;
    type UpdateError = Infallible;
    type RemoveError = Infallible;

    type Namespace = crypto::PublicKey;

    fn objects(
        &self,
        _typename: &radicle_cob::TypeName,
        _object_id: &radicle_cob::ObjectId,
    ) -> Result<radicle_cob::object::Objects, Self::ObjectsError> {
        todo!()
    }

    fn types(
        &self,
        _typename: &radicle_cob::TypeName,
    ) -> Result<
        std::collections::BTreeMap<radicle_cob::ObjectId, radicle_cob::object::Objects>,
        Self::TypesError,
    > {
        todo!()
    }

    fn update(
        &self,
        _namespace: &Self::Namespace,
        _typename: &radicle_cob::TypeName,
        _object_id: &radicle_cob::ObjectId,
        _entry: &radicle_cob::EntryId,
    ) -> Result<(), Self::UpdateError> {
        todo!()
    }

    fn remove(
        &self,
        _namespace: &Self::Namespace,
        _typename: &radicle_cob::TypeName,
        _object_id: &radicle_cob::ObjectId,
    ) -> Result<(), Self::RemoveError> {
        todo!()
    }
}

impl radicle_cob::change::Storage for MockRepository {
    type StoreError = radicle_cob::git::change::error::Create;
    type LoadError = radicle_cob::git::change::error::Load;
    type ObjectId = Oid;
    type Parent = Oid;
    type Signatures = radicle_cob::signatures::ExtendedSignature;

    fn store<G>(
        &self,
        _resource: Option<Self::Parent>,
        _related: Vec<Self::Parent>,
        _signer: &G,
        _template: radicle_cob::change::Template<Self::ObjectId>,
    ) -> Result<
        radicle_cob::change::store::Entry<Self::Parent, Self::ObjectId, Self::Signatures>,
        Self::StoreError,
    >
    where
        G: radicle_crypto::signature::Signer<Self::Signatures>,
    {
        todo!()
    }

    fn load(
        &self,
        _id: Self::ObjectId,
    ) -> Result<
        radicle_cob::change::store::Entry<Self::Parent, Self::ObjectId, Self::Signatures>,
        Self::LoadError,
    > {
        todo!()
    }

    fn parents_of(&self, _id: &Oid) -> Result<Vec<Oid>, Self::LoadError> {
        todo!()
    }

    fn manifest_of(&self, _id: &Oid) -> Result<radicle_cob::Manifest, Self::LoadError> {
        todo!()
    }
}

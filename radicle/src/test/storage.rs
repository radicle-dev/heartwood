use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use git_ext::ref_format as fmt;

use crate::crypto::{Signer, Verified};
use crate::identity::doc::{Doc, DocAt, DocError, RepoId};
use crate::node::NodeId;

pub use crate::storage::*;

use super::{arbitrary, fixtures};

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub path: PathBuf,
    pub info: git::UserInfo,

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

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }
}

impl ReadStorage for MockStorage {
    type Repository = MockRepository;

    fn info(&self) -> &git::UserInfo {
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

    fn inventory(&self) -> Result<Inventory, Error> {
        Ok(self.repos.keys().cloned().collect::<BTreeSet<_>>())
    }

    fn insert(&self, _rid: RepoId) {}

    fn repository(&self, rid: RepoId) -> Result<Self::Repository, RepositoryError> {
        self.repos
            .get(&rid)
            .ok_or_else(|| {
                RepositoryError::Storage(Error::Io(io::Error::from(io::ErrorKind::NotFound)))
            })
            .cloned()
    }

    fn repositories(&self) -> Result<Vec<RepositoryInfo<Verified>>, Error> {
        Ok(self
            .repos
            .iter()
            .map(|(rid, r)| RepositoryInfo {
                rid: *rid,
                head: r.head().unwrap().1,
                doc: r.doc.clone().into(),
                refs: None,
            })
            .collect())
    }
}

impl WriteStorage for MockStorage {
    type RepositoryMut = MockRepository;

    fn repository_mut(&self, rid: RepoId) -> Result<Self::RepositoryMut, RepositoryError> {
        self.repos
            .get(&rid)
            .ok_or(RepositoryError::Storage(Error::Io(io::Error::from(
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
    pub remotes: HashMap<NodeId, refs::SignedRefsAt>,
}

impl MockRepository {
    pub fn new(id: RepoId, doc: Doc<Verified>) -> Self {
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

impl RemoteRepository for MockRepository {
    fn remote(&self, id: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        self.remotes
            .get(id)
            .map(|refs| Remote {
                refs: refs.sigrefs.clone(),
            })
            .ok_or(refs::Error::InvalidRef)
    }

    fn remotes(&self) -> Result<Remotes<Verified>, refs::Error> {
        Ok(self
            .remotes
            .iter()
            .map(|(id, refs)| {
                (
                    *id,
                    Remote {
                        refs: refs.sigrefs.clone(),
                    },
                )
            })
            .collect())
    }

    fn remote_refs_at(&self) -> Result<Vec<refs::RefsAt>, refs::Error> {
        Ok(self
            .remotes
            .values()
            .map(|s| refs::RefsAt {
                remote: s.id,
                at: s.at,
            })
            .collect())
    }
}

impl ValidateRepository for MockRepository {
    fn validate_remote(&self, _remote: &Remote<Verified>) -> Result<Validations, Error> {
        Ok(Validations::default())
    }
}

impl ReadRepository for MockRepository {
    fn id(&self) -> RepoId {
        self.id
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(self.remotes.is_empty())
    }

    fn head(&self) -> Result<(fmt::Qualified, Oid), RepositoryError> {
        Ok((fmt::qualified!("refs/heads/master"), arbitrary::oid()))
    }

    fn canonical_head(&self) -> Result<(fmt::Qualified, Oid), RepositoryError> {
        todo!()
    }

    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn commit(&self, oid: Oid) -> Result<git2::Commit, git_ext::Error> {
        Err(git_ext::Error::NotFound(git_ext::NotFound::NoSuchObject(
            *oid,
        )))
    }

    fn revwalk(&self, _head: Oid) -> Result<git2::Revwalk, git2::Error> {
        todo!()
    }

    fn contains(&self, oid: Oid) -> Result<bool, git2::Error> {
        Ok(self
            .remotes
            .values()
            .any(|sigrefs| sigrefs.at == oid || sigrefs.refs.values().any(|oid_| *oid_ == oid)))
    }

    fn is_ancestor_of(&self, _ancestor: Oid, _head: Oid) -> Result<bool, git_ext::Error> {
        Ok(true)
    }

    fn blob(&self, _oid: Oid) -> Result<git2::Blob, git_ext::Error> {
        todo!()
    }

    fn blob_at<P: AsRef<std::path::Path>>(
        &self,
        _oid: git_ext::Oid,
        _path: P,
    ) -> Result<git2::Blob, git_ext::Error> {
        todo!()
    }

    fn reference(
        &self,
        _remote: &RemoteId,
        _reference: &git::Qualified,
    ) -> Result<git2::Reference, git_ext::Error> {
        todo!()
    }

    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &git::Qualified,
    ) -> Result<git_ext::Oid, git::raw::Error> {
        let not_found = || {
            git::raw::Error::new(
                git::raw::ErrorCode::NotFound,
                git::raw::ErrorClass::Reference,
                format!("could not find {reference} for {remote}"),
            )
        };

        let refs = self.remotes.get(remote).ok_or_else(not_found)?;
        if reference == &*refs::SIGREFS_BRANCH {
            Ok(refs.at)
        } else {
            refs.sigrefs.get(reference).ok_or_else(not_found)
        }
    }

    fn references_of(&self, _remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }

    fn references_glob(
        &self,
        _pattern: &git::PatternStr,
    ) -> Result<Vec<(fmt::Qualified, Oid)>, git::ext::Error> {
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

    fn identity_head_of(&self, _remote: &RemoteId) -> Result<Oid, git::ext::Error> {
        todo!()
    }

    fn identity_root(&self) -> Result<Oid, RepositoryError> {
        todo!()
    }

    fn identity_root_of(&self, _remote: &RemoteId) -> Result<Oid, RepositoryError> {
        todo!()
    }

    fn canonical_identity_head(&self) -> Result<Oid, RepositoryError> {
        Ok(Oid::from_str("cccccccccccccccccccccccccccccccccccccccc").unwrap())
    }

    fn merge_base(&self, _left: &Oid, _right: &Oid) -> Result<Oid, git::ext::Error> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn raw(&self) -> &git2::Repository {
        todo!()
    }

    fn set_head(&self) -> Result<SetHead, RepositoryError> {
        todo!()
    }

    fn set_identity_head_to(&self, _commit: Oid) -> Result<(), RepositoryError> {
        todo!()
    }

    fn set_user(&self, _info: &git::UserInfo) -> Result<(), Error> {
        todo!()
    }
}

impl SignRepository for MockRepository {
    fn sign_refs<G: Signer>(
        &self,
        _signer: &G,
    ) -> Result<crate::storage::refs::SignedRefs<Verified>, Error> {
        todo!()
    }
}

impl radicle_cob::Store for MockRepository {}

impl radicle_cob::object::Storage for MockRepository {
    type ObjectsError = Infallible;
    type TypesError = Infallible;
    type UpdateError = Infallible;
    type RemoveError = Infallible;

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
        _identifier: &radicle_crypto::PublicKey,
        _typename: &radicle_cob::TypeName,
        _object_id: &radicle_cob::ObjectId,
        _entry: &radicle_cob::EntryId,
    ) -> Result<(), Self::UpdateError> {
        todo!()
    }

    fn remove(
        &self,
        _identifier: &radicle_crypto::PublicKey,
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
        G: radicle_crypto::Signer,
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
}

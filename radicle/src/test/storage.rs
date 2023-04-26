use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use git_ref_format as fmt;
use radicle_git_ext as git_ext;

use crate::crypto::{Signer, Verified};
use crate::identity::doc::{Doc, DocError, Id};
use crate::identity::IdentityError;
use crate::node::NodeId;

pub use crate::storage::*;

#[derive(Clone, Debug)]
pub struct MockStorage {
    pub path: PathBuf,
    pub inventory: HashMap<Id, Doc<Verified>>,

    /// All refs keyed by RID.
    /// Each value is a map of refs keyed by node Id (public key).
    pub remotes: HashMap<Id, HashMap<NodeId, refs::SignedRefs<Verified>>>,
}

impl MockStorage {
    pub fn new(inventory: Vec<(Id, Doc<Verified>)>) -> Self {
        Self {
            path: PathBuf::default(),
            inventory: inventory.into_iter().collect(),
            remotes: HashMap::new(),
        }
    }

    pub fn empty() -> Self {
        Self {
            path: PathBuf::default(),
            inventory: HashMap::new(),
            remotes: HashMap::new(),
        }
    }

    /// Add a remote `node` with `signed_refs` for the repo `rid`.
    pub fn insert_remote(
        &mut self,
        rid: Id,
        node: NodeId,
        signed_refs: refs::SignedRefs<Verified>,
    ) {
        self.remotes
            .entry(rid)
            .or_insert(HashMap::new())
            .insert(node, signed_refs);
    }
}

impl ReadStorage for MockStorage {
    type Repository = MockRepository;

    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn path_of(&self, rid: &Id) -> PathBuf {
        self.path().join(rid.canonical())
    }

    fn contains(&self, rid: &Id) -> Result<bool, IdentityError> {
        Ok(self.inventory.contains_key(rid))
    }

    fn get(&self, _remote: &RemoteId, proj: Id) -> Result<Option<Doc<Verified>>, IdentityError> {
        Ok(self.inventory.get(&proj).cloned())
    }

    fn inventory(&self) -> Result<Inventory, Error> {
        Ok(self.inventory.keys().cloned().collect::<Vec<_>>())
    }

    fn repository(&self, rid: Id) -> Result<Self::Repository, Error> {
        let doc = self
            .inventory
            .get(&rid)
            .ok_or_else(|| Error::Io(io::Error::from(io::ErrorKind::NotFound)))?;
        Ok(MockRepository {
            id: rid,
            doc: doc.clone(),
            remotes: self.remotes.get(&rid).cloned().unwrap_or_default(),
        })
    }
}

impl WriteStorage for MockStorage {
    type RepositoryMut = MockRepository;

    fn repository_mut(&self, rid: Id) -> Result<Self::RepositoryMut, Error> {
        let doc = self.inventory.get(&rid).unwrap();
        Ok(MockRepository {
            id: rid,
            doc: doc.clone(),
            remotes: self.remotes.get(&rid).cloned().unwrap_or_default(),
        })
    }

    fn create(&self, _rid: Id) -> Result<Self::RepositoryMut, Error> {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct MockRepository {
    id: Id,
    doc: Doc<Verified>,
    remotes: HashMap<NodeId, refs::SignedRefs<Verified>>,
}

impl MockRepository {
    pub fn new(id: Id, doc: Doc<Verified>) -> Self {
        Self {
            id,
            doc,
            remotes: HashMap::default(),
        }
    }
}

impl ReadRepository for MockRepository {
    fn id(&self) -> Id {
        self.id
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        Ok(self.remotes.is_empty())
    }

    fn head(&self) -> Result<(fmt::Qualified, Oid), IdentityError> {
        todo!()
    }

    fn canonical_head(&self) -> Result<(fmt::Qualified, Oid), IdentityError> {
        todo!()
    }

    fn validate_remote(
        &self,
        _remote: &Remote<Verified>,
    ) -> Result<Vec<fmt::RefString>, VerifyError> {
        Ok(vec![])
    }

    fn path(&self) -> &std::path::Path {
        todo!()
    }

    fn remote(&self, id: &RemoteId) -> Result<Remote<Verified>, refs::Error> {
        self.remotes
            .get(id)
            .map(|refs| Remote {
                refs: refs.clone(),
                delegate: false,
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
                        refs: refs.clone(),
                        delegate: false,
                    },
                )
            })
            .collect())
    }

    fn commit(&self, _oid: Oid) -> Result<git2::Commit, git_ext::Error> {
        todo!()
    }

    fn revwalk(&self, _head: Oid) -> Result<git2::Revwalk, git2::Error> {
        todo!()
    }

    fn blob_at<'a>(
        &'a self,
        _oid: git_ext::Oid,
        _path: &'a std::path::Path,
    ) -> Result<git2::Blob<'a>, git_ext::Error> {
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
        _remote: &RemoteId,
        _reference: &git::Qualified,
    ) -> Result<git_ext::Oid, git_ext::Error> {
        todo!()
    }

    fn references_of(&self, _remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        todo!()
    }

    fn identity_doc(
        &self,
    ) -> Result<(Oid, crate::identity::Doc<crate::crypto::Unverified>), IdentityError> {
        Ok((git2::Oid::zero().into(), self.doc.clone().unverified()))
    }

    fn identity_doc_at(
        &self,
        _head: Oid,
    ) -> Result<crate::identity::Doc<crate::crypto::Unverified>, DocError> {
        Ok(self.doc.clone().unverified())
    }

    fn identity_head(&self) -> Result<Oid, IdentityError> {
        todo!()
    }

    fn canonical_identity_head(&self) -> Result<Oid, IdentityError> {
        todo!()
    }
}

impl WriteRepository for MockRepository {
    fn raw(&self) -> &git2::Repository {
        todo!()
    }

    fn set_head(&self) -> Result<Oid, IdentityError> {
        todo!()
    }

    fn sign_refs<G: Signer>(
        &self,
        _signer: &G,
    ) -> Result<crate::storage::refs::SignedRefs<Verified>, Error> {
        todo!()
    }

    fn set_identity_head(&self) -> Result<Oid, IdentityError> {
        todo!()
    }
}

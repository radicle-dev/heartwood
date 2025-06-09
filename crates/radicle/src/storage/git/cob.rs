//! COB storage Git backend.
use std::collections::BTreeMap;
use std::path::Path;

use cob::object::Objects;
use cob::signatures::ExtendedSignature;
use radicle_cob as cob;
use radicle_cob::change;
use storage::RemoteRepository;
use storage::RepositoryError;
use storage::SignRepository;
use storage::ValidateRepository;

use crate::git;
use crate::git::*;
use crate::identity;
use crate::identity::doc::DocError;
use crate::node::device::Device;
use crate::node::NodeId;
use crate::storage;
use crate::storage::Error;
use crate::storage::{
    git::{Remote, Remotes, Validations},
    ReadRepository, Verified,
};

use super::{RemoteId, Repository};

pub use crate::cob::{store, ObjectId, Store};

#[derive(Error, Debug)]
pub enum ObjectsError {
    #[error(transparent)]
    Convert(#[from] cob::object::storage::convert::Error),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    GitExt(#[from] git_ext::Error),
}

#[derive(Error, Debug)]
pub enum TypesError {
    #[error(transparent)]
    Convert(#[from] cob::object::storage::convert::Error),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    ParseKey(#[from] crypto::Error),
    #[error(transparent)]
    ParseObjectId(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    RefFormat(#[from] git::fmt::Error),
}

impl cob::Store for Repository {}

impl change::Storage for Repository {
    type StoreError = <git2::Repository as change::Storage>::StoreError;
    type LoadError = <git2::Repository as change::Storage>::LoadError;

    type ObjectId = <git2::Repository as change::Storage>::ObjectId;
    type Parent = <git2::Repository as change::Storage>::Parent;
    type Signatures = <git2::Repository as change::Storage>::Signatures;

    fn store<Signer>(
        &self,
        authority: Option<Self::Parent>,
        parents: Vec<Self::Parent>,
        signer: &Signer,
        spec: change::Template<Self::ObjectId>,
    ) -> Result<cob::Entry, Self::StoreError>
    where
        Signer: crypto::signature::Signer<ExtendedSignature>,
    {
        self.backend.store(authority, parents, signer, spec)
    }

    fn load(&self, id: Self::ObjectId) -> Result<cob::Entry, Self::LoadError> {
        self.backend.load(id)
    }

    fn parents_of(&self, id: &Oid) -> Result<Vec<Oid>, Self::LoadError> {
        self.backend.parents_of(id)
    }
}

impl cob::object::Storage for Repository {
    type ObjectsError = ObjectsError;
    type TypesError = TypesError;
    type UpdateError = git2::Error;
    type RemoveError = git2::Error;

    type Namespace = NodeId;

    fn objects(
        &self,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<cob::object::Objects, Self::ObjectsError> {
        let refs = self
            .backend
            .references_glob(git::refs::storage::cobs(typename, object_id).as_str())?;
        let refs = refs
            .map(|r| {
                r.map_err(Self::ObjectsError::from).and_then(|r| {
                    cob::object::Reference::try_from(r).map_err(Self::ObjectsError::from)
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(refs.into())
    }

    fn types(
        &self,
        typename: &cob::TypeName,
    ) -> Result<BTreeMap<cob::ObjectId, cob::object::Objects>, Self::TypesError> {
        // TODO: Use glob here.
        let mut references = self.backend.references()?.filter_map(|reference| {
            let reference = reference.ok()?;
            match RefStr::try_from_str(reference.name()?) {
                Ok(name) => {
                    let (ty, object_id) = cob::object::parse_refstr(&name)?;
                    if ty == *typename {
                        Some(
                            cob::object::Reference::try_from(reference)
                                .map_err(Self::TypesError::from)
                                .map(|reference| (object_id, reference)),
                        )
                    } else {
                        None
                    }
                }
                Err(err) => Some(Err(err.into())),
            }
        });

        references.try_fold(BTreeMap::new(), |mut objects, result| {
            let (oid, reference) = result?;
            objects
                .entry(oid)
                .and_modify(|objs: &mut cob::object::Objects| objs.push(reference.clone()))
                .or_insert_with(|| cob::object::Objects::new(reference));
            Ok(objects)
        })
    }

    fn update(
        &self,
        namespace: &Self::Namespace,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
        entry: &cob::EntryId,
    ) -> Result<(), Self::UpdateError> {
        self.backend.reference(
            git::refs::storage::cob(namespace, typename, object_id).as_str(),
            (*entry).into(),
            true,
            &format!(
                "Updating collaborative object '{}/{}' with new entry {}",
                typename, object_id, entry,
            ),
        )?;

        Ok(())
    }

    fn remove(
        &self,
        namespace: &Self::Namespace,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<(), Self::RemoveError> {
        let mut reference = self
            .backend
            .find_reference(git::refs::storage::cob(namespace, typename, object_id).as_str())?;

        reference.delete()
    }
}

/// Stores draft collaborative objects.
///
// This storage backend for COBs stores changes in a `draft/cobs/*` namespace,
// which allows for some of the features needed for code review. For
// example, users can draft comments and later decide to publish them.
pub struct DraftStore<'a, R> {
    repo: &'a R,
    remote: RemoteId,
}

impl<'a, R> DraftStore<'a, R> {
    pub fn new(repo: &'a R, remote: RemoteId) -> Self {
        Self { repo, remote }
    }
}

impl<R: storage::WriteRepository> cob::Store for DraftStore<'_, R> {}

impl<R: storage::WriteRepository> change::Storage for DraftStore<'_, R> {
    type StoreError = <git2::Repository as change::Storage>::StoreError;
    type LoadError = <git2::Repository as change::Storage>::LoadError;

    type ObjectId = <git2::Repository as change::Storage>::ObjectId;
    type Parent = <git2::Repository as change::Storage>::Parent;
    type Signatures = <git2::Repository as change::Storage>::Signatures;

    fn store<Signer>(
        &self,
        authority: Option<Self::Parent>,
        parents: Vec<Self::Parent>,
        signer: &Signer,
        spec: change::Template<Self::ObjectId>,
    ) -> Result<cob::Entry, Self::StoreError>
    where
        Signer: crypto::signature::Signer<ExtendedSignature>,
    {
        self.repo.raw().store(authority, parents, signer, spec)
    }

    fn load(&self, id: Self::ObjectId) -> Result<cob::Entry, Self::LoadError> {
        self.repo.raw().load(id)
    }

    fn parents_of(&self, id: &Oid) -> Result<Vec<Oid>, Self::LoadError> {
        self.repo.raw().parents_of(id)
    }
}

impl<R> SignRepository for DraftStore<'_, R>
where
    R: storage::ReadRepository,
{
    fn sign_refs<G: crypto::signature::Signer<crypto::Signature>>(
        &self,
        signer: &Device<G>,
    ) -> Result<storage::refs::SignedRefs<Verified>, RepositoryError> {
        // Since this is a draft store, we do not actually want to sign the refs.
        // Instead, we just return the existing signed refs.
        let remote = self.repo.remote(signer.public_key())?;

        Ok(remote.refs)
    }
}

impl<R: storage::RemoteRepository> RemoteRepository for DraftStore<'_, R> {
    fn remote(&self, id: &RemoteId) -> Result<Remote<Verified>, storage::refs::Error> {
        self.repo.remote(id)
    }

    fn remotes(&self) -> Result<Remotes<Verified>, storage::refs::Error> {
        RemoteRepository::remotes(self.repo)
    }

    fn remote_refs_at(&self) -> Result<Vec<storage::refs::RefsAt>, storage::refs::Error> {
        RemoteRepository::remote_refs_at(self.repo)
    }
}

impl<R: storage::ValidateRepository> ValidateRepository for DraftStore<'_, R> {
    fn validate_remote(&self, remote: &Remote<Verified>) -> Result<Validations, Error> {
        self.repo.validate_remote(remote)
    }
}

impl<R: storage::ReadRepository> ReadRepository for DraftStore<'_, R> {
    fn id(&self) -> identity::RepoId {
        self.repo.id()
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        self.repo.is_empty()
    }

    fn head(&self) -> Result<(Qualified, Oid), RepositoryError> {
        self.repo.head()
    }

    fn canonical_head(&self) -> Result<(Qualified, Oid), RepositoryError> {
        self.repo.canonical_head()
    }

    fn path(&self) -> &std::path::Path {
        self.repo.path()
    }

    fn commit(&self, oid: Oid) -> Result<git2::Commit, git_ext::Error> {
        self.repo.commit(oid)
    }

    fn revwalk(&self, head: Oid) -> Result<git2::Revwalk, git2::Error> {
        self.repo.revwalk(head)
    }

    fn contains(&self, oid: Oid) -> Result<bool, raw::Error> {
        self.repo.contains(oid)
    }

    fn is_ancestor_of(&self, ancestor: Oid, head: Oid) -> Result<bool, git_ext::Error> {
        self.repo.is_ancestor_of(ancestor, head)
    }

    fn blob_at<P: AsRef<Path>>(
        &self,
        oid: git_ext::Oid,
        path: P,
    ) -> Result<git2::Blob, git_ext::Error> {
        self.repo.blob_at(oid, path)
    }

    fn blob(&self, oid: git_ext::Oid) -> Result<raw::Blob, ext::Error> {
        self.repo.blob(oid)
    }

    fn reference(
        &self,
        remote: &RemoteId,
        reference: &git::Qualified,
    ) -> Result<git2::Reference, git_ext::Error> {
        self.repo.reference(remote, reference)
    }

    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &git::Qualified,
    ) -> Result<git_ext::Oid, git::raw::Error> {
        self.repo.reference_oid(remote, reference)
    }

    fn references_of(&self, remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        self.repo.references_of(remote)
    }

    fn references_glob(
        &self,
        pattern: &git::PatternStr,
    ) -> Result<Vec<(fmt::Qualified, Oid)>, git::ext::Error> {
        self.repo.references_glob(pattern)
    }

    fn identity_doc(&self) -> Result<crate::identity::DocAt, RepositoryError> {
        self.repo.identity_doc()
    }

    fn identity_doc_at(&self, head: Oid) -> Result<crate::identity::DocAt, DocError> {
        self.repo.identity_doc_at(head)
    }

    fn identity_head(&self) -> Result<Oid, RepositoryError> {
        self.repo.identity_head()
    }

    fn identity_head_of(&self, remote: &RemoteId) -> Result<Oid, super::ext::Error> {
        self.repo.identity_head_of(remote)
    }

    fn identity_root(&self) -> Result<Oid, RepositoryError> {
        self.repo.identity_root()
    }

    fn identity_root_of(&self, remote: &RemoteId) -> Result<Oid, RepositoryError> {
        self.repo.identity_root_of(remote)
    }

    fn canonical_identity_head(&self) -> Result<Oid, RepositoryError> {
        self.repo.canonical_identity_head()
    }

    fn merge_base(&self, left: &Oid, right: &Oid) -> Result<Oid, git::ext::Error> {
        self.repo.merge_base(left, right)
    }
}

impl<R: storage::WriteRepository> cob::object::Storage for DraftStore<'_, R> {
    type ObjectsError = ObjectsError;
    type TypesError = git::ext::Error;
    type UpdateError = git2::Error;
    type RemoveError = git2::Error;

    type Namespace = NodeId;

    fn objects(
        &self,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<cob::object::Objects, Self::ObjectsError> {
        let mut objs = Objects::default();

        // First get the signed COB tips of all remotes for this object.
        let signed = self
            .repo
            .references_glob(&git::refs::storage::cobs(typename, object_id))?;

        for (r, id) in signed {
            let r = cob::object::Reference {
                name: r.to_ref_string(),
                target: cob::object::Commit { id },
            };
            objs.push(r);
        }

        // Then get the draft COB tip that belongs to us only, if any.
        let draft_ref = &git::refs::storage::draft::cob(&self.remote, typename, object_id);
        if let Ok(draft_oid) = self
            .repo
            .reference_oid(&self.remote, &draft_ref.strip_namespace())
        {
            objs.push(cob::object::Reference {
                name: draft_ref.to_ref_string(),
                target: cob::object::Commit { id: draft_oid },
            })
        }

        Ok(objs)
    }

    fn types(
        &self,
        _typename: &cob::TypeName,
    ) -> Result<BTreeMap<cob::ObjectId, cob::object::Objects>, Self::TypesError> {
        unimplemented!()
    }

    fn update(
        &self,
        namespace: &Self::Namespace,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
        entry: &cob::history::EntryId,
    ) -> Result<(), Self::UpdateError> {
        self.repo.raw().reference(
            git::refs::storage::draft::cob(namespace, typename, object_id).as_str(),
            (*entry).into(),
            true,
            &format!(
                "Updating draft collaborative object '{}/{}' with new entry {}",
                typename, object_id, entry,
            ),
        )?;

        Ok(())
    }

    fn remove(
        &self,
        namespace: &Self::Namespace,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<(), Self::RemoveError> {
        let mut reference = self.repo.raw().find_reference(
            git::refs::storage::draft::cob(namespace, typename, object_id).as_str(),
        )?;

        reference.delete()
    }
}

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
use crate::git::fmt::*;
use crate::git::*;
use crate::identity;
use crate::identity::doc::DocError;
use crate::node::NodeId;
use crate::storage;
use crate::storage::Error;
use crate::storage::{
    ReadRepository,
    git::{Remote, Remotes, Validations},
};

use super::{RemoteId, Repository};

pub use crate::cob::{ObjectId, Store, store};

#[derive(Error, Debug)]
pub enum ObjectsError {
    #[error(transparent)]
    Convert(#[from] cob::object::storage::convert::Error),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
}

#[derive(Error, Debug)]
pub enum TypesError {
    #[error(transparent)]
    Convert(#[from] cob::object::storage::convert::Error),
    #[error(transparent)]
    Git(#[from] git::raw::Error),
    #[error(transparent)]
    ParseKey(#[from] crypto::Error),
    #[error(transparent)]
    ParseObjectId(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    RefFormat(#[from] git::fmt::Error),
}

impl cob::Store for Repository {}

impl change::Storage for Repository {
    type StoreError = <git::raw::Repository as change::Storage>::StoreError;
    type LoadError = <git::raw::Repository as change::Storage>::LoadError;

    type ObjectId = <git::raw::Repository as change::Storage>::ObjectId;
    type Parent = <git::raw::Repository as change::Storage>::Parent;
    type Signatures = <git::raw::Repository as change::Storage>::Signatures;

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

    fn manifest_of(&self, id: &Oid) -> Result<cob::Manifest, Self::LoadError> {
        self.backend.manifest_of(id)
    }
}

impl cob::object::Storage for Repository {
    type ObjectsError = ObjectsError;
    type TypesError = TypesError;
    type UpdateError = git::raw::Error;
    type RemoveError = git::raw::Error;

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
                "Updating collaborative object '{typename}/{object_id}' with new entry {entry}",
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
    type StoreError = <git::raw::Repository as change::Storage>::StoreError;
    type LoadError = <git::raw::Repository as change::Storage>::LoadError;

    type ObjectId = <git::raw::Repository as change::Storage>::ObjectId;
    type Parent = <git::raw::Repository as change::Storage>::Parent;
    type Signatures = <git::raw::Repository as change::Storage>::Signatures;

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

    fn manifest_of(&self, id: &Oid) -> Result<cob::Manifest, Self::LoadError> {
        self.repo.raw().manifest_of(id)
    }
}

impl<Repo> SignRepository for DraftStore<'_, Repo>
where
    Repo: storage::ReadRepository,
{
    fn sign_refs<Signer>(
        &self,
        signer: &Signer,
    ) -> Result<storage::refs::SignedRefs, RepositoryError>
    where
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
        Signer: crypto::signature::Verifier<crypto::Signature>,
    {
        // Since this is a draft store, we do not actually want to sign the refs.
        // Instead, we just return the existing signed refs.
        let remote = self.repo.remote(&signer.verifying_key())?;

        Ok(remote.refs)
    }

    fn force_sign_refs<Signer>(
        &self,
        signer: &Signer,
    ) -> Result<storage::refs::SignedRefs, RepositoryError>
    where
        Signer: crypto::signature::Keypair<VerifyingKey = crypto::PublicKey>,
        Signer: crypto::signature::Signer<crypto::Signature>,
        Signer: crypto::signature::Verifier<crypto::Signature>,
    {
        self.sign_refs(signer)
    }
}

impl<R: storage::RemoteRepository> RemoteRepository for DraftStore<'_, R> {
    fn remote(&self, id: &RemoteId) -> Result<Remote, storage::refs::Error> {
        self.repo.remote(id)
    }

    fn remotes(&self) -> Result<Remotes, storage::refs::Error> {
        RemoteRepository::remotes(self.repo)
    }

    fn remote_refs_at(&self) -> Result<Vec<storage::refs::RefsAt>, storage::refs::Error> {
        RemoteRepository::remote_refs_at(self.repo)
    }
}

impl<R: storage::ValidateRepository> ValidateRepository for DraftStore<'_, R> {
    fn validate_remote(&self, remote: &Remote) -> Result<Validations, Error> {
        self.repo.validate_remote(remote)
    }
}

impl<R: storage::ReadRepository> ReadRepository for DraftStore<'_, R> {
    fn id(&self) -> identity::RepoId {
        self.repo.id()
    }

    fn is_empty(&self) -> Result<bool, git::raw::Error> {
        self.repo.is_empty()
    }

    fn head(&self) -> Result<(Qualified<'_>, Oid), RepositoryError> {
        self.repo.head()
    }

    fn canonical_head(&self) -> Result<(Qualified<'_>, Oid), RepositoryError> {
        self.repo.canonical_head()
    }

    fn path(&self) -> &std::path::Path {
        self.repo.path()
    }

    fn commit(&self, oid: Oid) -> Result<git::raw::Commit<'_>, git::raw::Error> {
        self.repo.commit(oid)
    }

    fn revwalk(&self, head: Oid) -> Result<git::raw::Revwalk<'_>, git::raw::Error> {
        self.repo.revwalk(head)
    }

    fn contains(&self, oid: Oid) -> Result<bool, crate::git::raw::Error> {
        self.repo.contains(oid)
    }

    fn is_ancestor_of(&self, ancestor: Oid, head: Oid) -> Result<bool, crate::git::raw::Error> {
        self.repo.is_ancestor_of(ancestor, head)
    }

    fn blob_at<P: AsRef<Path>>(
        &self,
        oid: Oid,
        path: P,
    ) -> Result<git::raw::Blob<'_>, git::raw::Error> {
        self.repo.blob_at(oid, path)
    }

    fn blob(&self, oid: Oid) -> Result<crate::git::raw::Blob<'_>, crate::git::raw::Error> {
        self.repo.blob(oid)
    }

    fn reference(
        &self,
        remote: &RemoteId,
        reference: &git::fmt::Qualified,
    ) -> Result<git::raw::Reference<'_>, git::raw::Error> {
        self.repo.reference(remote, reference)
    }

    fn reference_oid(
        &self,
        remote: &RemoteId,
        reference: &git::fmt::Qualified,
    ) -> Result<Oid, crate::git::raw::Error> {
        self.repo.reference_oid(remote, reference)
    }

    fn references_of(&self, remote: &RemoteId) -> Result<crate::storage::refs::Refs, Error> {
        self.repo.references_of(remote)
    }

    fn references_glob(
        &self,
        pattern: &git::fmt::refspec::PatternStr,
    ) -> Result<Vec<(fmt::Qualified<'_>, Oid)>, crate::git::raw::Error> {
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

    fn identity_head_of(&self, remote: &RemoteId) -> Result<Oid, crate::git::raw::Error> {
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

    fn merge_base(&self, left: &Oid, right: &Oid) -> Result<Oid, crate::git::raw::Error> {
        self.repo.merge_base(left, right)
    }
}

impl<R: storage::WriteRepository> cob::object::Storage for DraftStore<'_, R> {
    type ObjectsError = ObjectsError;
    type TypesError = git::raw::Error;
    type UpdateError = git::raw::Error;
    type RemoveError = git::raw::Error;

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
                "Updating draft collaborative object '{typename}/{object_id}' with new entry {entry}",
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

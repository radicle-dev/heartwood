//! COB storage Git backend.
use std::collections::BTreeMap;

use cob::object::Objects;
use radicle_cob as cob;
use radicle_cob::change;
use storage::SignRepository;

use crate::git::*;
use crate::storage;
use crate::storage::Error;
use crate::storage::{
    git::{Remote, Remotes, VerifyError},
    ReadRepository, Verified,
};
use crate::{
    git, identity,
    identity::{doc::DocError, IdentityError},
};

use super::{RemoteId, Repository};

#[derive(Error, Debug)]
pub enum ObjectsError {
    #[error(transparent)]
    Convert(#[from] cob::object::storage::convert::Error),
    #[error(transparent)]
    Git(#[from] git2::Error),
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
        authority: Self::Parent,
        parents: Vec<Self::Parent>,
        signer: &Signer,
        spec: change::Template<Self::ObjectId>,
    ) -> Result<cob::Change, Self::StoreError>
    where
        Signer: crypto::Signer,
    {
        self.backend.store(authority, parents, signer, spec)
    }

    fn load(&self, id: Self::ObjectId) -> Result<cob::Change, Self::LoadError> {
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

    type Identifier = RemoteId;

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
        identifier: &Self::Identifier,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
        change: &cob::Change,
    ) -> Result<(), Self::UpdateError> {
        self.backend.reference(
            git::refs::storage::cob(identifier, typename, object_id).as_str(),
            (*change.id()).into(),
            true,
            &format!(
                "Updating collaborative object '{}/{}' with new change {}",
                typename,
                object_id,
                change.id()
            ),
        )?;

        Ok(())
    }

    fn remove(
        &self,
        identifier: &Self::Identifier,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<(), Self::RemoveError> {
        let mut reference = self
            .backend
            .find_reference(git::refs::storage::cob(identifier, typename, object_id).as_str())?;

        reference.delete().map_err(Self::RemoveError::from)
    }
}

/// Stores draft collaborative objects.
///
// This storage backend for COBs stores changes in a `draft/cobs/*` namespace,
// which allows for some of the features needed for code review. For
// example, users can draft comments and later decide to publish them.
pub struct DraftStore<'a> {
    remote: RemoteId,
    repo: &'a Repository,
}

impl<'a> DraftStore<'a> {
    pub fn new(remote: RemoteId, repo: &'a Repository) -> Self {
        Self { remote, repo }
    }
}

impl<'a> cob::Store for DraftStore<'a> {}

impl<'a> change::Storage for DraftStore<'a> {
    type StoreError = <git2::Repository as change::Storage>::StoreError;
    type LoadError = <git2::Repository as change::Storage>::LoadError;

    type ObjectId = <git2::Repository as change::Storage>::ObjectId;
    type Parent = <git2::Repository as change::Storage>::Parent;
    type Signatures = <git2::Repository as change::Storage>::Signatures;

    fn store<Signer>(
        &self,
        authority: Self::Parent,
        parents: Vec<Self::Parent>,
        signer: &Signer,
        spec: change::Template<Self::ObjectId>,
    ) -> Result<cob::Change, Self::StoreError>
    where
        Signer: crypto::Signer,
    {
        self.repo.backend.store(authority, parents, signer, spec)
    }

    fn load(&self, id: Self::ObjectId) -> Result<cob::Change, Self::LoadError> {
        self.repo.backend.load(id)
    }

    fn parents_of(&self, id: &Oid) -> Result<Vec<Oid>, Self::LoadError> {
        self.repo.backend.parents_of(id)
    }
}

impl<'a> SignRepository for DraftStore<'a> {
    fn sign_refs<G: crypto::Signer>(
        &self,
        signer: &G,
    ) -> Result<storage::refs::SignedRefs<Verified>, Error> {
        self.repo.sign_refs(signer)
    }
}

impl<'a> ReadRepository for DraftStore<'a> {
    fn id(&self) -> identity::Id {
        self.repo.id()
    }

    fn is_empty(&self) -> Result<bool, git2::Error> {
        self.repo.is_empty()
    }

    fn head(&self) -> Result<(fmt::Qualified, Oid), identity::IdentityError> {
        self.repo.head()
    }

    fn canonical_head(&self) -> Result<(fmt::Qualified, Oid), identity::IdentityError> {
        self.repo.canonical_head()
    }

    fn validate_remote(
        &self,
        remote: &Remote<Verified>,
    ) -> Result<Vec<fmt::RefString>, VerifyError> {
        self.repo.validate_remote(remote)
    }

    fn path(&self) -> &std::path::Path {
        self.repo.path()
    }

    fn remote(&self, id: &RemoteId) -> Result<Remote<Verified>, storage::refs::Error> {
        self.repo.remote(id)
    }

    fn remotes(&self) -> Result<Remotes<Verified>, storage::refs::Error> {
        ReadRepository::remotes(self.repo)
    }

    fn commit(&self, oid: Oid) -> Result<git2::Commit, git_ext::Error> {
        self.repo.commit(oid)
    }

    fn revwalk(&self, head: Oid) -> Result<git2::Revwalk, git2::Error> {
        self.repo.revwalk(head)
    }

    fn is_ancestor_of(&self, ancestor: Oid, head: Oid) -> Result<bool, git_ext::Error> {
        self.repo.is_ancestor_of(ancestor, head)
    }

    fn blob_at<'b>(
        &'b self,
        oid: git_ext::Oid,
        path: &'b std::path::Path,
    ) -> Result<git2::Blob<'b>, git_ext::Error> {
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
    ) -> Result<git_ext::Oid, git_ext::Error> {
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

    fn identity_doc(
        &self,
    ) -> Result<(Oid, crate::identity::Doc<crate::crypto::Unverified>), IdentityError> {
        self.repo.identity_doc()
    }

    fn identity_doc_at(
        &self,
        head: Oid,
    ) -> Result<crate::identity::Doc<crate::crypto::Unverified>, DocError> {
        self.repo.identity_doc_at(head)
    }

    fn identity_head(&self) -> Result<Oid, IdentityError> {
        self.repo.identity_head()
    }

    fn canonical_identity_head(&self) -> Result<Oid, IdentityError> {
        self.repo.canonical_identity_head()
    }

    fn merge_base(&self, left: &Oid, right: &Oid) -> Result<Oid, git::ext::Error> {
        self.repo.merge_base(left, right)
    }
}

impl<'a> cob::object::Storage for DraftStore<'a> {
    type ObjectsError = ObjectsError;
    type TypesError = git::ext::Error;
    type UpdateError = git2::Error;
    type RemoveError = git2::Error;

    type Identifier = RemoteId;

    fn objects(
        &self,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<cob::object::Objects, Self::ObjectsError> {
        // Nb. There can only be one draft per COB, per remote.
        let Ok(r) = self.repo.backend.find_reference(
            git::refs::storage::draft::cob(&self.remote, typename, object_id).as_str(),
        ) else {
            return Ok(Objects::default());
        };
        let r = cob::object::Reference::try_from(r).map_err(Self::ObjectsError::from)?;

        Ok(Objects::new(r))
    }

    fn types(
        &self,
        typename: &cob::TypeName,
    ) -> Result<BTreeMap<cob::ObjectId, cob::object::Objects>, Self::TypesError> {
        let glob = git::refs::storage::draft::cobs(&self.remote, typename);
        let references = self.repo.references_glob(&glob)?;
        let mut objs = BTreeMap::new();

        for (name, id) in references {
            let r = cob::object::Reference {
                name: name.into_refstring(),
                target: cob::object::Commit { id },
            };
            objs.insert(id.into(), Objects::new(r));
        }
        Ok(objs)
    }

    fn update(
        &self,
        identifier: &Self::Identifier,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
        change: &cob::Change,
    ) -> Result<(), Self::UpdateError> {
        self.repo.backend.reference(
            git::refs::storage::draft::cob(identifier, typename, object_id).as_str(),
            (*change.id()).into(),
            true,
            &format!(
                "Updating draft collaborative object '{}/{}' with new change {}",
                typename,
                object_id,
                change.id()
            ),
        )?;

        Ok(())
    }

    fn remove(
        &self,
        identifier: &Self::Identifier,
        typename: &cob::TypeName,
        object_id: &cob::ObjectId,
    ) -> Result<(), Self::RemoveError> {
        let mut reference = self.repo.backend.find_reference(
            git::refs::storage::draft::cob(identifier, typename, object_id).as_str(),
        )?;

        reference.delete()
    }
}

//! COB storage Git backend.
use std::collections::BTreeMap;

use radicle_cob as cob;
use radicle_cob::change;

use crate::git;
use crate::git::fmt::*;
use crate::git::*;
use crate::node::NodeId;
use crate::storage::Error;

use super::Repository;

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
    ParseObjectId(#[from] cob::object::ParseObjectId),
    #[error(transparent)]
    RefFormat(#[from] git::fmt::Error),
}

/// Convert a Git reference into a COB reference.
///
/// A reference that does not point to a commit is skipped with a warning, so that
/// one bad reference from any peer does not make the object or type unreadable.
fn cob_reference(
    reference: git::raw::Reference,
) -> Result<Option<cob::object::Reference>, cob::object::storage::convert::Error> {
    match cob::object::Reference::try_from(reference) {
        Ok(reference) => Ok(Some(reference)),
        Err(err @ cob::object::storage::convert::Error::NotCommit { .. }) => {
            log::warn!(target: "storage", "Skipping invalid COB reference: {err}");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

impl cob::Store for Repository {}

impl change::Storage for Repository {
    type StoreError = <git::raw::Repository as change::Storage>::StoreError;
    type LoadError = <git::raw::Repository as change::Storage>::LoadError;

    type ObjectId = <git::raw::Repository as change::Storage>::ObjectId;
    type Parent = <git::raw::Repository as change::Storage>::Parent;

    type PublicKey = <git::raw::Repository as change::Storage>::PublicKey;
    type Signature = <git::raw::Repository as change::Storage>::Signature;

    fn store(
        &self,
        authority: Option<Self::Parent>,
        parents: Vec<Self::Parent>,
        signer: &impl crypto::Signer,
        spec: change::Template<Self::ObjectId>,
    ) -> Result<cob::Entry, Self::StoreError> {
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
        self.backend
            .references_glob(&git::refs::storage::cob(typename, object_id))?
            .try_fold(cob::object::Objects::default(), |mut objects, reference| {
                if let Some(reference) = cob_reference(reference?)? {
                    objects.push(reference);
                }
                Ok(objects)
            })
    }

    fn types(
        &self,
        typename: &cob::TypeName,
    ) -> Result<BTreeMap<cob::ObjectId, cob::object::Objects>, Self::TypesError> {
        self.backend
            .references_glob(&git::refs::storage::cobs(typename))?
            .filter_map(Result::ok)
            .filter_map(
                |reference| match RefStr::try_from_str(reference.name().ok()?) {
                    Ok(name) => {
                        let (ty, object_id) = cob::object::parse_refstr(&name)?;
                        debug_assert_eq!(ty, *typename);
                        Some(Ok((object_id, reference)))
                    }
                    Err(err) => Some(Err(err.into())),
                },
            )
            .filter_map(
                |result: Result<(ObjectId, git2::Reference<'_>), Self::TypesError>| match result {
                    Ok((object_id, reference)) => match cob_reference(reference) {
                        Ok(Some(reference)) => {
                            Some(Ok::<_, Self::TypesError>((object_id, reference)))
                        }
                        Ok(None) => None,
                        Err(err) => Some(Err(err.into())),
                    },
                    Err(err) => Some(Err(err)),
                },
            )
            .try_fold(BTreeMap::new(), |mut objects, result| {
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
            git::refs::storage::cob_with_namespace(namespace, typename, object_id).as_str(),
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
        let mut reference = self.backend.find_reference(
            git::refs::storage::cob_with_namespace(namespace, typename, object_id).as_str(),
        )?;

        reference.delete()
    }
}

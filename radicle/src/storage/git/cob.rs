//! COB storage Git backend.
use std::collections::HashMap;

use radicle_cob as cob;
use radicle_cob::change;

use crate::git;
use crate::storage::Error;

pub use crate::git::*;
pub use cob::*;

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
    RefFormat(#[from] git_ref_format::Error),
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
    ) -> Result<HashMap<cob::ObjectId, cob::object::Objects>, Self::TypesError> {
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

        references.try_fold(HashMap::new(), |mut objects, result| {
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

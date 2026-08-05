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
            match RefStr::try_from_str(reference.name().ok()?) {
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

use std::{collections::BTreeMap, convert::TryFrom as _};

use radicle_crypto::PublicKey;
use tempfile::TempDir;

use crate::{
    change,
    object::{self, Reference},
    signatures, ObjectId, Store,
};

pub mod error {
    use thiserror::Error;

    use crate::object::storage::convert;

    #[derive(Debug, Error)]
    pub enum Identity {
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
    }

    #[derive(Debug, Error)]
    pub enum Objects {
        #[error(transparent)]
        Conversion(#[from] convert::Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Format(#[from] git_ext::ref_format::Error),
    }
}

pub struct Storage {
    raw: git2::Repository,
    _temp: TempDir,
}

impl Storage {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let raw = git2::Repository::init(temp.path()).unwrap();
        let mut config = raw.config().unwrap();
        config.set_str("user.name", "Terry Pratchett").unwrap();
        config
            .set_str("user.email", "http://www.gnuterrypratchett.com")
            .unwrap();
        Self { raw, _temp: temp }
    }

    pub fn as_raw(&self) -> &git2::Repository {
        &self.raw
    }
}

impl Store for Storage {}

impl change::Storage for Storage {
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
    ) -> Result<
        change::store::Entry<Self::Parent, Self::ObjectId, Self::Signatures>,
        Self::StoreError,
    >
    where
        Signer: signature::Signer<signatures::ExtendedSignature>,
    {
        self.as_raw().store(authority, parents, signer, spec)
    }

    fn load(
        &self,
        id: Self::ObjectId,
    ) -> Result<change::store::Entry<Self::Parent, Self::ObjectId, Self::Signatures>, Self::LoadError>
    {
        self.as_raw().load(id)
    }

    fn parents_of(&self, id: &git_ext::Oid) -> Result<Vec<git_ext::Oid>, Self::LoadError> {
        Ok(self
            .as_raw()
            .find_commit(**id)?
            .parent_ids()
            .map(git_ext::Oid::from)
            .collect::<Vec<_>>())
    }
}

impl object::Storage for Storage {
    type ObjectsError = error::Objects;
    type TypesError = error::Objects;
    type UpdateError = git2::Error;
    type RemoveError = git2::Error;

    fn objects(
        &self,
        typename: &crate::TypeName,
        object_id: &ObjectId,
    ) -> Result<object::Objects, Self::ObjectsError> {
        let glob = format!("refs/rad/*/cobs/{typename}/{object_id}");
        let remotes = self
            .raw
            .references_glob(&glob)?
            .map(|r| {
                r.map_err(error::Objects::from)
                    .and_then(|r| Reference::try_from(r).map_err(error::Objects::from))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(remotes.into())
    }

    fn types(
        &self,
        typename: &crate::TypeName,
    ) -> Result<BTreeMap<ObjectId, object::Objects>, Self::TypesError> {
        let mut objects = BTreeMap::new();
        for r in self.raw.references_glob("refs/rad/*")? {
            let r = r?;
            let name = r.name().unwrap();
            println!("NAME: {name}");
            let oid = r
                .target()
                .map(ObjectId::from)
                .expect("BUG: the cob references should be direct");
            if name.contains(typename.as_str()) {
                let reference = Reference::try_from(r)?;
                objects
                    .entry(oid)
                    .and_modify(|objs: &mut object::Objects| objs.push(reference.clone()))
                    .or_insert_with(|| object::Objects::new(reference));
            }
        }
        Ok(objects)
    }

    fn update(
        &self,
        identifier: &PublicKey,
        typename: &crate::TypeName,
        object_id: &ObjectId,
        entry: &change::EntryId,
    ) -> Result<(), Self::UpdateError> {
        let name = format!("refs/rad/{}/cobs/{}/{}", identifier, typename, object_id);
        self.raw
            .reference(&name, (*entry).into(), true, "new change")?;
        Ok(())
    }

    fn remove(
        &self,
        identifier: &PublicKey,
        typename: &crate::TypeName,
        object_id: &ObjectId,
    ) -> Result<(), Self::RemoveError> {
        let name = format!("refs/rad/{}/cobs/{}/{}", identifier, typename, object_id);
        self.raw.find_reference(&name)?.delete()?;

        Ok(())
    }
}

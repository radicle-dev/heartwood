use std::{collections::HashMap, convert::TryFrom as _};

use git_ref_format::RefString;
use tempfile::TempDir;

use crate::{
    change,
    object::{self, Commit, Reference},
    ObjectId, Store,
};

use super::identity::{RemoteProject, Urn};

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Identity {
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("'identity' was not a blob, found '{0:?}'")]
        NotBlob(Option<git2::ObjectType>),
        #[error("could not find 'identity' in the tree '{0}'")]
        NotFound(git_ext::Oid),
    }

    #[derive(Debug, Error)]
    pub enum Objects {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Format(#[from] git_ref_format::Error),
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

impl Store<RemoteProject> for Storage {}

impl change::Storage for Storage {
    type CreateError = <git2::Repository as change::Storage>::CreateError;
    type LoadError = <git2::Repository as change::Storage>::LoadError;

    type ObjectId = <git2::Repository as change::Storage>::ObjectId;
    type Author = <git2::Repository as change::Storage>::Author;
    type Resource = <git2::Repository as change::Storage>::Resource;
    type Signatures = <git2::Repository as change::Storage>::Signatures;

    fn create<Signer>(
        &self,
        author: Option<Self::Author>,
        authority: Self::Resource,
        signer: &Signer,
        spec: change::Create<Self::ObjectId>,
    ) -> Result<
        change::store::Change<Self::Author, Self::Resource, Self::ObjectId, Self::Signatures>,
        Self::CreateError,
    >
    where
        Signer: crypto::Signer,
    {
        self.as_raw().create(author, authority, signer, spec)
    }

    fn load(
        &self,
        id: Self::ObjectId,
    ) -> Result<
        change::store::Change<Self::Author, Self::Resource, Self::ObjectId, Self::Signatures>,
        Self::LoadError,
    > {
        self.as_raw().load(id)
    }
}

impl object::Storage for Storage {
    type ObjectsError = error::Objects;
    type TypesError = error::Objects;
    type UpdateError = git2::Error;

    type Identifier = Urn;

    fn objects(
        &self,
        identifier: &Self::Identifier,
        typename: &crate::TypeName,
        object_id: &ObjectId,
    ) -> Result<object::Objects, Self::ObjectsError> {
        let name = format!(
            "refs/rad/{}/cobs/{}/{}",
            identifier.to_path(),
            typename,
            object_id
        );
        let glob = format!(
            "refs/rad/{}/*/cobs/{}/{}",
            identifier.name.as_str(),
            typename,
            object_id
        );
        let local = {
            let r = self.raw.find_reference(&name)?;
            Some(resolve_reference(r)?)
        };
        let remotes = self
            .raw
            .references_glob(&glob)?
            .map(|r| r.map_err(error::Objects::from).and_then(resolve_reference))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(object::Objects { local, remotes })
    }

    fn types(
        &self,
        identifier: &Self::Identifier,
        typename: &crate::TypeName,
    ) -> Result<HashMap<ObjectId, object::Objects>, Self::TypesError> {
        let mut objects = HashMap::new();
        let prefix = format!("refs/rad/{}/cobs/{}", identifier.to_path(), typename);
        for r in self.raw.references()? {
            let r = r?;
            let name = r.name().unwrap();
            let oid = r
                .target()
                .map(ObjectId::from)
                .expect("BUG: the cob references should be direct");
            if name.starts_with(&prefix) {
                objects.insert(
                    oid,
                    object::Objects {
                        local: Some(resolve_reference(r)?),
                        remotes: Vec::new(),
                    },
                );
            }
        }
        Ok(objects)
    }

    fn update(
        &self,
        identifier: &Self::Identifier,
        typename: &crate::TypeName,
        object_id: &ObjectId,
        change: &change::Change,
    ) -> Result<(), Self::UpdateError> {
        let name = format!(
            "refs/rad/{}/cobs/{}/{}",
            identifier.to_path(),
            typename,
            object_id
        );
        let id = *change.id();
        self.raw.reference(&name, id.into(), true, "new change")?;
        Ok(())
    }
}

fn resolve_reference(r: git2::Reference) -> Result<Reference, error::Objects> {
    let commit = r.peel_to_commit()?;
    let target = resolve_parents(commit)?;
    Ok(Reference {
        name: RefString::try_from(r.name().unwrap().to_owned())?,
        target,
    })
}

fn resolve_parents(commit: git2::Commit) -> Result<Commit, git2::Error> {
    let parents = commit
        .parents()
        .map(resolve_parents)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Commit {
        id: commit.id().into(),
        parents,
    })
}

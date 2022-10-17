use git_ext::Oid;
use serde::{Deserialize, Serialize};

use crate::identity::Identity;
use crate::test::storage::{self, Storage};

use super::{Name, Urn};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub payload: Payload,
    pub content_id: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    name: Name,
    key: crypto::PublicKey,
}

impl Person {
    pub fn new(
        repo: &Storage,
        name: &str,
        key: crypto::PublicKey,
    ) -> Result<Self, storage::error::Identity> {
        let repo = repo.as_raw();
        let refname = format!("refs/rad/identities/{}", name);
        let payload = Payload {
            name: Name(name.to_owned()),
            key,
        };
        let blob = serde_json::to_vec(&payload)?;
        let oid = repo.blob(&blob)?;
        let mut tree = repo.treebuilder(None)?;
        tree.insert("identity", oid, git2::FileMode::Blob.into())?;
        let oid = tree.write()?;
        let tree = repo.find_tree(oid)?;
        let signature = git2::Signature::now(name, name)?;
        let content_id = repo
            .commit(
                Some(&refname),
                &signature,
                &signature,
                "persisted identity",
                &tree,
                &[],
            )?
            .into();
        Ok(Self {
            payload,
            content_id,
        })
    }

    pub fn key(&self) -> crypto::PublicKey {
        self.payload.key
    }

    pub fn name(&self) -> &Name {
        &self.payload.name
    }

    pub fn find_by_oid(
        repo: &git2::Repository,
        id: Oid,
    ) -> Result<Option<Person>, storage::error::Identity> {
        match repo.find_commit(id.into()) {
            Ok(commit) => from_commit(repo, commit),
            Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

fn from_commit(
    repo: &git2::Repository,
    commit: git2::Commit,
) -> Result<Option<Person>, storage::error::Identity> {
    let tree = commit.tree()?;
    let entry = tree
        .get_name("identity")
        .ok_or_else(|| storage::error::Identity::NotFound(tree.id().into()))?;
    let blob = match entry.to_object(repo)?.into_blob() {
        Ok(blob) => blob,
        Err(other) => return Err(storage::error::Identity::NotBlob(other.kind())),
    };
    let payload = serde_json::de::from_slice(blob.content())?;
    Ok(Some(Person {
        payload,
        content_id: commit.id().into(),
    }))
}

impl Identity for Person {
    type Identifier = Urn;

    fn is_delegate(&self, delegation: &crypto::PublicKey) -> bool {
        self.key() == *delegation
    }

    fn content_id(&self) -> Oid {
        self.content_id
    }

    fn identifier(&self) -> Self::Identifier {
        Urn {
            name: self.name().clone(),
            remote: None,
        }
    }
}

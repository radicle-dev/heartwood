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
        let refname = format!("refs/rad/identities/{name}");
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

    pub fn name(&self) -> &Name {
        &self.payload.name
    }
}

impl Identity for Person {
    type Identifier = Urn;

    fn content_id(&self) -> Oid {
        self.content_id
    }
}

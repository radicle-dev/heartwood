use std::collections::BTreeSet;

use git_ext::Oid;
use serde::{Deserialize, Serialize};

use crate::identity::Identity;
use crate::test;
use crate::test::storage::{self, Storage};

use super::{Name, Urn};

pub struct RemoteProject {
    pub project: Project,
    pub person: test::Person,
}

impl RemoteProject {
    pub fn identifier(&self) -> Urn {
        Urn {
            name: self.project.name().clone(),
            remote: Some(self.person.name().clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub payload: Payload,
    pub content_id: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    name: Name,
    delegates: BTreeSet<crypto::PublicKey>,
}

impl Project {
    pub fn new(
        repo: &Storage,
        name: &str,
        delegate: crypto::PublicKey,
    ) -> Result<Self, storage::error::Identity> {
        let repo = repo.as_raw();
        let refname = format!("refs/rad/identities/{name}");
        let payload = Payload {
            name: Name(name.to_owned()),
            delegates: Some(delegate).into_iter().collect(),
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

impl Identity for RemoteProject {
    type Identifier = Urn;

    fn content_id(&self) -> Oid {
        self.project.content_id
    }
}

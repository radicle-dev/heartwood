use localtime::LocalTime;
use radicle::prelude::*;

use crate::git;
use radicle::git::RefStr;
use radicle::node::policy::Scope;
use radicle::node::{routing, Handle, NodeId};
use radicle::Node;

/// Setup a repository remote and tracking branch.
pub struct SetupRemote<'a> {
    /// The repository id.
    pub rid: RepoId,
    /// Whether or not to setup a remote tracking branch.
    pub tracking: Option<BranchName>,
    /// Whether or not to fetch the remote immediately.
    pub fetch: bool,
    /// The repository in which to setup the remote.
    pub repo: &'a git::Repository,
}

impl<'a> SetupRemote<'a> {
    /// Run the setup for the given peer.
    pub fn run(
        &self,
        name: impl AsRef<RefStr>,
        node: NodeId,
    ) -> anyhow::Result<(git::Remote, Option<BranchName>)> {
        let remote_url = radicle::git::Url::from(self.rid).with_namespace(node);
        let remote_name = name.as_ref();

        if git::is_remote(self.repo, remote_name)? {
            anyhow::bail!("remote `{remote_name}` already exists");
        }

        let remote =
            radicle::git::configure_remote(self.repo, remote_name, &remote_url, &remote_url)?;
        let mut remote = git::Remote::try_from(remote)?;

        // Fetch the refs into the working copy.
        if self.fetch {
            remote.fetch::<&str>(&[], None, None)?;
        }
        // Setup remote-tracking branch.
        if let Some(branch) = &self.tracking {
            let tracking_branch = remote_name.join(branch);
            let local_branch = radicle::git::refs::workdir::branch(tracking_branch.as_refstr());
            radicle::git::set_upstream(self.repo, remote_name, &tracking_branch, local_branch)?;

            return Ok((remote, Some(tracking_branch)));
        }
        Ok((remote, None))
    }
}

/// Add the repo to our inventory.
pub fn add_inventory(
    rid: RepoId,
    node: &mut Node,
    profile: &Profile,
) -> Result<bool, anyhow::Error> {
    match node.add_inventory(rid) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let now = LocalTime::now();
            let mut db = profile.database_mut()?;
            let updates =
                routing::Store::add_inventory(&mut db, [&rid], *profile.id(), now.into())?;

            Ok(!updates.is_empty())
        }
        Err(e) => Err(e.into()),
    }
}

/// Seed a repository by first trying to seed through the node, and if the node isn't running, by
/// updating the policy database directly. If the repo is available locally, we also add it to our
/// inventory.
pub fn seed(
    rid: RepoId,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<bool, anyhow::Error> {
    match node.seed(rid, scope) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let mut config = profile.policies_mut()?;
            let result = config.seed(&rid, scope)?;

            if result && profile.storage.contains(&rid)? {
                let now = LocalTime::now();
                let mut db = profile.database_mut()?;

                routing::Store::add_inventory(&mut db, [&rid], *profile.id(), now.into())?;
            }
            Ok(result)
        }
        Err(e) => Err(e.into()),
    }
}

/// Unseed a repository by first trying to unseed through the node, and if the node isn't running,
/// by updating the policy database directly.
pub fn unseed(rid: RepoId, node: &mut Node, profile: &Profile) -> Result<bool, anyhow::Error> {
    match node.unseed(rid) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let mut config = profile.policies_mut()?;
            let result = config.unseed(&rid)?;

            let mut db = profile.database_mut()?;
            radicle::node::routing::Store::remove_inventory(&mut db, &rid, profile.id())?;

            Ok(result)
        }
        Err(e) => Err(e.into()),
    }
}

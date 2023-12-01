use radicle::prelude::*;

use crate::git;
use radicle::git::RefStr;
use radicle::node::policy::Scope;
use radicle::node::{Handle, NodeId};
use radicle::Node;

/// Setup a project remote and tracking branch.
pub struct SetupRemote<'a> {
    /// The project id.
    pub rid: Id,
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

/// Seed a repository by first trying to seed through the node, and if the node isn't running,
/// by updating the policy database directly.
pub fn seed(
    rid: Id,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<bool, anyhow::Error> {
    match node.seed(rid, scope) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let mut config = profile.policies_mut()?;
            config.seed(&rid, scope).map_err(|e| e.into())
        }
        Err(e) => Err(e.into()),
    }
}

/// Unseed a repository by first trying to unseed through the node, and if the node isn't running,
/// by updating the policy database directly.
pub fn unseed(rid: Id, node: &mut Node, profile: &Profile) -> Result<bool, anyhow::Error> {
    match node.unseed(rid) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let mut config = profile.policies_mut()?;
            config.unseed(&rid).map_err(|e| e.into())
        }
        Err(e) => Err(e.into()),
    }
}

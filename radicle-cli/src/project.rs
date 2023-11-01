use radicle::prelude::*;

use crate::git;
use radicle::git::RefStr;
use radicle::node::tracking::Scope;
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

/// Track a repository by first trying to track through the node, and if the node isn't running,
/// by updating the tracking database directly.
pub fn track(
    rid: Id,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<bool, anyhow::Error> {
    match node.track_repo(rid, scope) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let mut config = profile.tracking_mut()?;
            config.track_repo(&rid, scope).map_err(|e| e.into())
        }
        Err(e) => Err(e.into()),
    }
}

/// Untrack a repository by first trying to untrack through the node, and if the node isn't running,
/// by updating the tracking database directly.
pub fn untrack(rid: Id, node: &mut Node, profile: &Profile) -> Result<bool, anyhow::Error> {
    match node.untrack_repo(rid) {
        Ok(updated) => Ok(updated),
        Err(e) if e.is_connection_err() => {
            let mut config = profile.tracking_mut()?;
            config.untrack_repo(&rid).map_err(|e| e.into())
        }
        Err(e) => Err(e.into()),
    }
}

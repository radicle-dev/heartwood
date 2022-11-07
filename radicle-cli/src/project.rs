use radicle::git::raw::Remote;
use radicle::prelude::*;
use radicle::rad;

use crate::git;

/// Setup a project remote and tracking branch.
pub struct SetupRemote<'a> {
    /// The project id.
    pub project: Id,
    /// The project default branch.
    pub default_branch: BranchName,
    /// The repository in which to setup the remote.
    pub repo: &'a git::Repository,
    /// Whether or not to fetch the remote immediately.
    pub fetch: bool,
    /// Whether or not to setup a remote tracking branch.
    pub tracking: bool,
}

impl<'a> SetupRemote<'a> {
    /// Run the setup for the given peer.
    pub fn run(&self, node: NodeId) -> anyhow::Result<Option<(Remote, String)>> {
        let url = radicle::git::Url::from(self.project).with_namespace(node);
        let mut remote =
            radicle::git::configure_remote(self.repo, rad::peer_remote(&node).as_str(), &url)?;

        // Fetch the refs into the working copy.
        if self.fetch {
            remote.fetch::<&str>(&[], None, None)?;
        }
        // Setup remote-tracking branch.
        if self.tracking {
            let branch = git::set_tracking(self.repo, &node, &self.default_branch)?;

            return Ok(Some((remote, branch)));
        }
        Ok(None)
    }
}

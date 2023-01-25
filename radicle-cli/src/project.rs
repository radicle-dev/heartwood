use radicle::git::raw::Remote;
use radicle::git::RefString;
use radicle::prelude::*;

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
    pub fn run(&self, node: NodeId) -> anyhow::Result<Option<(Remote, RefString)>> {
        let url = radicle::git::Url::from(self.project).with_namespace(node);
        let mut remote = radicle::git::configure_remote(self.repo, &node.to_string(), &url)?;

        // Fetch the refs into the working copy.
        if self.fetch {
            remote.fetch::<&str>(&[], None, None)?;
        }
        // Setup remote-tracking branch.
        if self.tracking {
            // SAFETY: Node IDs are valid ref strings.
            let node_ref = RefString::try_from(node.to_string()).unwrap();
            let node_ref = node_ref.as_refstr();
            let branch_name = node_ref.join(&self.default_branch);
            let local_branch = radicle::git::refs::workdir::branch(
                node_ref.join(&self.default_branch).as_refstr(),
            );
            radicle::git::set_upstream(self.repo, &node.to_string(), &branch_name, &local_branch)?;

            return Ok(Some((remote, branch_name)));
        }
        Ok(None)
    }
}

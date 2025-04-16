use radicle::prelude::*;

use crate::git;
use radicle::git::RefStr;
use radicle::node::NodeId;

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

impl SetupRemote<'_> {
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

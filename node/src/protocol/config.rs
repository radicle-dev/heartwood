use std::net;

use git_url::Url;

use crate::collections::HashSet;
use crate::identity::{ProjId, UserId};
use crate::protocol::message::{Address, Envelope, Message};

/// Peer-to-peer network.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Network {
    #[default]
    Main,
    Test,
}

impl Network {
    pub fn magic(&self) -> u32 {
        match self {
            Self::Main => 0x819b43d9,
            Self::Test => 0x717ebaf8,
        }
    }

    pub fn envelope(&self, msg: Message) -> Envelope {
        Envelope {
            magic: self.magic(),
            msg,
        }
    }
}

/// Project tracking policy.
#[derive(Debug, Clone)]
pub enum ProjectTracking {
    /// Track all projects we come across.
    All { blocked: HashSet<ProjId> },
    /// Track a static list of projects.
    Allowed(HashSet<ProjId>),
}

impl Default for ProjectTracking {
    fn default() -> Self {
        Self::All {
            blocked: HashSet::default(),
        }
    }
}

/// Project remote tracking policy.
#[derive(Debug, Default, Clone)]
pub enum RemoteTracking {
    /// Only track remotes of project delegates.
    #[default]
    DelegatesOnly,
    /// Track all remotes.
    All { blocked: HashSet<UserId> },
    /// Track a specific list of users as well as the project delegates.
    Allowed(HashSet<UserId>),
}

/// Protocol configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Peers to connect to on startup.
    /// Connections to these peers will be maintained.
    pub connect: Vec<net::SocketAddr>,
    /// Peer-to-peer network.
    pub network: Network,
    /// Project tracking policy.
    pub project_tracking: ProjectTracking,
    /// Project remote tracking policy.
    pub remote_tracking: RemoteTracking,
    /// Whether or not our node should relay inventories.
    pub relay: bool,
    /// List of addresses to listen on for protocol connections.
    pub listen: Vec<Address>,
    /// Our Git URL for fetching projects.
    pub git_url: Url,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect: Vec::default(),
            network: Network::default(),
            project_tracking: ProjectTracking::default(),
            remote_tracking: RemoteTracking::default(),
            relay: true,
            listen: vec![],
            git_url: Url::default(),
        }
    }
}

impl Config {
    pub fn is_persistent(&self, addr: &net::SocketAddr) -> bool {
        self.connect.contains(addr)
    }

    pub fn is_tracking(&self, proj: &ProjId) -> bool {
        match &self.project_tracking {
            ProjectTracking::All { blocked } => !blocked.contains(proj),
            ProjectTracking::Allowed(projs) => projs.contains(proj),
        }
    }

    /// Track a project. Returns whether the policy was updated.
    pub fn track(&mut self, proj: ProjId) -> bool {
        match &mut self.project_tracking {
            ProjectTracking::All { .. } => false,
            ProjectTracking::Allowed(projs) => projs.insert(proj),
        }
    }

    /// Untrack a project. Returns whether the policy was updated.
    pub fn untrack(&mut self, proj: ProjId) -> bool {
        match &mut self.project_tracking {
            ProjectTracking::All { blocked } => blocked.insert(proj),
            ProjectTracking::Allowed(projs) => projs.remove(&proj),
        }
    }
}

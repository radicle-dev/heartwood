use std::io::BufRead as _;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io, iter, net, process, thread, time,
    time::Duration,
};

use crossbeam_channel as chan;

use radicle::cob;
use radicle::cob::issue;
use radicle::crypto::ssh::keystore::MemorySigner;
use radicle::crypto::test::signer::MockSigner;
use radicle::crypto::Signer;
use radicle::git;
use radicle::git::refname;
use radicle::identity::{RepoId, Visibility};
use radicle::node::config::ConnectAddress;
use radicle::node::policy::store as policy;
use radicle::node::seed::Store as _;
use radicle::node::Config;
use radicle::node::{self, Alias};
use radicle::node::{ConnectOptions, Handle as _};
use radicle::node::{Database, POLICIES_DB_FILE};
use radicle::profile::{env, Home, Profile};
use radicle::rad;
use radicle::storage::{ReadStorage as _, RemoteRepository as _, SignRepository as _};
use radicle::test::fixtures;
use radicle::Storage;

use crate::node::NodeId;
use crate::service::Event;
use crate::storage::git::transport;
use crate::{runtime, runtime::Handle, service, Runtime};

/// A node that can be run.
pub struct Node<G> {
    pub id: NodeId,
    pub home: Home,
    pub signer: G,
    pub storage: Storage,
    pub config: Config,
    pub db: service::Stores<Database>,
    pub policies: policy::Store<policy::Write>,
}

impl Node<MemorySigner> {
    pub fn new(profile: Profile) -> Self {
        let signer = MemorySigner::load(&profile.keystore, None).unwrap();
        let id = *profile.id();
        let policies_db = profile.home.node().join(POLICIES_DB_FILE);
        let policies = policy::Store::open(policies_db).unwrap();
        let db = profile.database_mut().unwrap();
        let db = service::Stores::from(db);

        Node {
            id,
            home: profile.home,
            config: profile.config.node,
            signer,
            db,
            policies,
            storage: profile.storage,
        }
    }
}

/// Handle to a running node.
pub struct NodeHandle<G: 'static> {
    pub id: NodeId,
    pub alias: Alias,
    pub storage: Storage,
    pub signer: G,
    pub home: Home,
    pub addr: net::SocketAddr,
    pub thread: ManuallyDrop<thread::JoinHandle<Result<(), runtime::Error>>>,
    pub handle: ManuallyDrop<Handle>,
}

impl<G: 'static> Drop for NodeHandle<G> {
    fn drop(&mut self) {
        log::debug!(target: "test", "Node {} shutting down..", self.id);

        unsafe { ManuallyDrop::take(&mut self.handle) }
            .shutdown()
            .unwrap();
        unsafe { ManuallyDrop::take(&mut self.thread) }
            .join()
            .unwrap()
            .unwrap();
    }
}

impl<G: Signer + cyphernet::Ecdh> NodeHandle<G> {
    /// Connect this node to another node, and wait for the connection to be established both ways.
    pub fn connect(&mut self, remote: &NodeHandle<G>) -> &mut Self {
        let local_events = self.handle.events();
        let remote_events = remote.handle.events();

        self.handle
            .connect(remote.id, remote.addr.into(), ConnectOptions::default())
            .ok();

        local_events
            .iter()
            .find(|e| {
                matches!(
                    e, Event::PeerConnected { nid } if nid == &remote.id
                )
            })
            .unwrap();
        remote_events
            .iter()
            .find(|e| {
                matches!(
                    e, Event::PeerConnected { nid } if nid == &self.id
                )
            })
            .unwrap();

        self
    }

    pub fn disconnect(&mut self, remote: &NodeHandle<G>) {
        self.handle.disconnect(remote.id).unwrap();
    }

    /// Shutdown node.
    pub fn shutdown(self) {
        drop(self)
    }

    /// Get the full address of this node.
    pub fn address(&self) -> ConnectAddress {
        (self.id, node::Address::from(self.addr)).into()
    }

    /// Get routing table entries.
    pub fn routing(&self) -> impl Iterator<Item = (RepoId, NodeId)> {
        use node::routing::Store as _;

        self.home.routing_mut().unwrap().entries().unwrap()
    }

    pub fn inventory(&self) -> impl Iterator<Item = RepoId> + '_ {
        self.routing()
            .filter(|(_, n)| *n == self.id)
            .map(|(r, _)| r)
    }

    /// Get sync status of a repo.
    pub fn synced_seeds(&self, rid: &RepoId) -> Vec<node::seed::SyncedSeed> {
        let db = Database::reader(self.home.node().join(node::NODE_DB_FILE)).unwrap();
        let seeds = db.seeds_for(rid).unwrap();

        seeds.into_iter().collect::<Result<Vec<_>, _>>().unwrap()
    }

    /// Wait until this node's routing table matches the remotes.
    pub fn converge<'a>(
        &'a self,
        remotes: impl IntoIterator<Item = &'a NodeHandle<G>>,
    ) -> BTreeSet<(RepoId, NodeId)> {
        converge(iter::once(self).chain(remotes))
    }

    /// Wait until this node's routing table contains the given routes.
    #[track_caller]
    pub fn routes_to(&self, routes: &[(RepoId, NodeId)]) {
        log::debug!(target: "test", "Waiting for {} to route to {:?}", self.id, routes);
        let events = self.handle.events();

        loop {
            let mut remaining: BTreeSet<_> = routes.iter().collect();

            for (rid, nid) in self.routing() {
                if !remaining.remove(&(rid, nid)) {
                    log::debug!(target: "test", "Found unexpected route for {}: ({rid}, {nid})", self.id);
                }
            }
            if remaining.is_empty() {
                break;
            }
            events
                .wait(
                    |e| matches!(e, Event::SeedDiscovered { .. }).then_some(()),
                    time::Duration::from_secs(6),
                )
                .unwrap();
        }
    }

    /// Wait until this node is synced with another node, for the given repository.
    #[track_caller]
    pub fn is_synced_with(&mut self, rid: &RepoId, nid: &NodeId) {
        log::debug!(target: "test", "Waiting for {} to be in sync with {nid} for {rid}", self.id);

        loop {
            let seeds = self.handle.seeds(*rid).unwrap();
            if seeds.iter().any(|s| s.nid == *nid && s.is_synced()) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Wait until this node has a repository.
    #[track_caller]
    pub fn has_repository(&self, rid: &RepoId) {
        log::debug!(target: "test", "Waiting for {} to have {rid}", self.id);
        let events = self.handle.events();

        loop {
            if self.storage.repository(*rid).is_ok() {
                log::debug!(target: "test", "Node {} has {rid}", self.id);
                break;
            }
            events
                .wait(
                    |e| matches!(e, Event::RefsFetched { .. }).then_some(()),
                    time::Duration::from_secs(6),
                )
                .unwrap();
        }
    }

    /// Wait until this node has the inventory of another node.
    #[track_caller]
    pub fn has_remote_of(&self, rid: &RepoId, nid: &NodeId) {
        log::debug!(target: "test", "Waiting for {} to have {rid}/{nid}", self.id);
        let events = self.handle.events();

        loop {
            if let Ok(repo) = self.storage.repository(*rid) {
                if repo.remote(nid).is_ok() {
                    log::debug!(target: "test", "Node {} has {rid}/{nid}", self.id);
                    break;
                }
            }
            events
                .wait(
                    |e| matches!(e, Event::RefsFetched { .. }).then_some(()),
                    time::Duration::from_secs(6),
                )
                .unwrap();
        }
    }

    /// Clone a repo into a directory.
    pub fn clone<P: AsRef<Path>>(&self, rid: RepoId, cwd: P) -> io::Result<()> {
        self.rad("clone", &[rid.to_string().as_str()], cwd)
    }

    /// Fork a repo.
    pub fn fork<P: AsRef<Path>>(&self, rid: RepoId, cwd: P) -> io::Result<()> {
        self.clone(rid, &cwd)?;
        self.rad("fork", &[rid.to_string().as_str()], &cwd)?;
        self.announce(rid, 1, &cwd)?;

        Ok(())
    }

    /// Announce a repo.
    pub fn announce<P: AsRef<Path>>(&self, rid: RepoId, replicas: usize, cwd: P) -> io::Result<()> {
        self.rad(
            "sync",
            &[
                rid.to_string().as_str(),
                "--announce",
                "--replicas",
                replicas.to_string().as_str(),
            ],
            cwd,
        )
    }

    /// Init a repo.
    pub fn init<P: AsRef<Path>>(&self, name: &str, desc: &str, cwd: P) -> io::Result<()> {
        self.rad(
            "init",
            &[
                "--name",
                name,
                "--description",
                desc,
                "--default-branch",
                "master",
                "--public",
            ],
            cwd,
        )
    }

    /// Run a `rad` CLI command.
    pub fn rad<P: AsRef<Path>>(&self, cmd: &str, args: &[&str], cwd: P) -> io::Result<()> {
        let cwd = cwd.as_ref();
        log::debug!(target: "test", "Running `rad {cmd} {args:?}` in {}..", cwd.display());

        fs::create_dir_all(cwd)?;

        let result = process::Command::new(snapbox::cmd::cargo_bin("rad"))
            .env_clear()
            .envs(env::vars().filter(|(k, _)| k == "PATH"))
            .env("GIT_AUTHOR_DATE", "1671125284")
            .env("GIT_AUTHOR_EMAIL", "radicle@localhost")
            .env("GIT_AUTHOR_NAME", "radicle")
            .env("GIT_COMMITTER_DATE", "1671125284")
            .env("GIT_COMMITTER_EMAIL", "radicle@localhost")
            .env("GIT_COMMITTER_NAME", "radicle")
            .env(
                env::RAD_HOME,
                self.home.path().to_string_lossy().to_string(),
            )
            .env(env::RAD_PASSPHRASE, "radicle")
            .env(env::RAD_LOCAL_TIME, "1671125284")
            .env("TZ", "UTC")
            .env("LANG", "C")
            .envs(git::env::GIT_DEFAULT_CONFIG)
            .current_dir(cwd)
            .arg(cmd)
            .args(args)
            .output()?;

        for line in io::BufReader::new(io::Cursor::new(&result.stdout))
            .lines()
            .map_while(Result::ok)
        {
            log::debug!(target: "test", "rad {cmd}: {line}");
        }

        log::debug!(
            target: "test",
            "Ran command `rad {cmd}` (status={})", result.status.code().unwrap()
        );

        if !result.status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(())
    }

    /// Create an [`issue::Issue`] in the `NodeHandle`'s storage.
    pub fn issue(&self, rid: RepoId, title: &str, desc: &str) -> cob::ObjectId {
        let repo = self.storage.repository(rid).unwrap();
        let mut issues = issue::Cache::no_cache(&repo).unwrap();
        *issues
            .create(title, desc, &[], &[], [], &self.signer)
            .unwrap()
            .id()
    }
}

impl Node<MockSigner> {
    /// Create a new node.
    pub fn init(base: &Path, config: Config) -> Self {
        let home = base.join(
            iter::repeat_with(fastrand::alphanumeric)
                .take(8)
                .collect::<String>(),
        );
        let home = Home::new(home).unwrap();
        let signer = MockSigner::default();
        let storage = Storage::open(
            home.storage(),
            git::UserInfo {
                alias: config.alias.clone(),
                key: *signer.public_key(),
            },
        )
        .unwrap();
        let policies = home.policies_mut().unwrap();
        let db = home.database_mut().unwrap();
        let db = service::Stores::from(db);

        log::debug!(target: "test", "Node::init {}: {}", config.alias, signer.public_key());
        Self {
            id: *signer.public_key(),
            home,
            signer,
            storage,
            config,
            db,
            policies,
        }
    }
}

impl<G: cyphernet::Ecdh<Pk = NodeId> + Signer + Clone> Node<G> {
    /// Spawn a node in its own thread.
    pub fn spawn(self) -> NodeHandle<G> {
        let alias = self.config.alias.clone();
        let listen = vec![([0, 0, 0, 0], 0).into()];
        let (_, signals) = chan::bounded(1);
        let rt = Runtime::init(
            self.home.clone(),
            self.config,
            listen,
            signals,
            self.signer.clone(),
        )
        .unwrap();
        let addr = *rt.local_addrs.first().unwrap();
        let id = *self.signer.public_key();
        let handle = ManuallyDrop::new(rt.handle.clone());
        let thread = ManuallyDrop::new(runtime::thread::spawn(&id, "runtime", move || rt.run()));

        NodeHandle {
            id,
            alias,
            storage: self.storage,
            signer: self.signer,
            home: self.home,
            addr,
            handle,
            thread,
        }
    }

    /// Populate a storage instance with a project from the given repository.
    pub fn project_from(
        &mut self,
        name: &str,
        description: &str,
        repo: &git::raw::Repository,
    ) -> RepoId {
        transport::local::register(self.storage.clone());

        let branch = refname!("master");
        let id = rad::init(
            repo,
            name.try_into().unwrap(),
            description,
            branch.clone(),
            Visibility::default(),
            &self.signer,
            &self.storage,
        )
        .map(|(id, _, _)| id)
        .unwrap();

        assert!(self.policies.seed(&id, node::policy::Scope::All).unwrap());

        log::debug!(
            target: "test",
            "Initialized project {id} for node {}", self.signer.public_key()
        );

        // Push local branches to storage.
        let mut refs = Vec::<(git::Qualified, git::Qualified)>::new();
        for branch in repo.branches(Some(git::raw::BranchType::Local)).unwrap() {
            let (branch, _) = branch.unwrap();
            let name = git::RefString::try_from(branch.name().unwrap().unwrap()).unwrap();

            refs.push((
                git::lit::refs_heads(&name).into(),
                git::lit::refs_heads(&name).into(),
            ));
        }
        git::push(repo, "rad", refs.iter().map(|(a, b)| (a, b))).unwrap();

        radicle::git::set_upstream(
            repo,
            &*radicle::rad::REMOTE_NAME,
            branch.clone(),
            radicle::git::refs::workdir::branch(&branch),
        )
        .unwrap();

        self.storage
            .repository(id)
            .unwrap()
            .sign_refs(&self.signer)
            .unwrap();

        id
    }

    /// Populate a storage instance with a project.
    pub fn project(&mut self, name: &str, description: &str) -> RepoId {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = fixtures::repository(tmp.path());

        self.project_from(name, description, &repo)
    }
}

/// Checks whether the nodes have converged in their routing tables.
#[track_caller]
pub fn converge<'a, G: Signer + cyphernet::Ecdh + 'static>(
    nodes: impl IntoIterator<Item = &'a NodeHandle<G>>,
) -> BTreeSet<(RepoId, NodeId)> {
    let nodes = nodes.into_iter().collect::<Vec<_>>();

    let mut all_routes = BTreeSet::<(RepoId, NodeId)>::new();
    let mut remaining = BTreeMap::from_iter(nodes.iter().map(|node| (node.id, node)));

    // First build the set of all routes.
    for node in &nodes {
        // Routes from the routing table.
        for (rid, seed_id) in node.routing() {
            all_routes.insert((rid, seed_id));
        }
        // Routes from the local inventory.
        for rid in node.inventory() {
            all_routes.insert((rid, node.id));
        }
    }

    // Then, while there are nodes remaining to converge, check each node to see if
    // its routing table has all routes. If so, remove it from the remaining nodes.
    while !remaining.is_empty() {
        remaining.retain(|_, node| {
            let routing = node.routing();
            let routes = BTreeSet::from_iter(routing);

            if routes.is_superset(&all_routes) {
                log::debug!(target: "test", "Node {} has converged", node.id);
                return false;
            } else {
                let diff = all_routes.symmetric_difference(&routes).collect::<Vec<_>>();
                log::debug!(target: "test", "Node has missing routes: {diff:?}");
            }
            true
        });
        thread::sleep(Duration::from_millis(100));
    }
    all_routes
}

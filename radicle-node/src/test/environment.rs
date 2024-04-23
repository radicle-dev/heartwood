use std::io::BufRead as _;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs, io, iter, net, process, thread, time,
    time::Duration,
};

use crossbeam_channel as chan;

use radicle::cob::cache::COBS_DB_FILE;
use radicle::cob::issue;
use radicle::crypto::ssh::{keystore::MemorySigner, Keystore};
use radicle::crypto::test::signer::MockSigner;
use radicle::crypto::{KeyPair, Seed, Signer};
use radicle::git::refname;
use radicle::identity::{RepoId, Visibility};
use radicle::node::config::ConnectAddress;
use radicle::node::policy::store as policy;
use radicle::node::routing::Store;
use radicle::node::seed::Store as _;
use radicle::node::Database;
use radicle::node::{Alias, POLICIES_DB_FILE};
use radicle::node::{ConnectOptions, Handle as _};
use radicle::profile;
use radicle::profile::{Home, Profile};
use radicle::rad;
use radicle::storage::{ReadStorage as _, RemoteRepository as _, SignRepository as _};
use radicle::test::fixtures;
use radicle::Storage;
use radicle::{cli, node};
use radicle::{cob, explorer};
use radicle::{git, web};

use crate::node::NodeId;
use crate::service::Event;
use crate::storage::git::transport;
use crate::{runtime, runtime::Handle, service, Runtime};

pub use service::Config;

/// Test environment.
pub struct Environment {
    tempdir: tempfile::TempDir,
    users: usize,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            tempdir: tempfile::tempdir().unwrap(),
            users: 0,
        }
    }
}

impl Environment {
    /// Create a new test environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the temp directory path.
    pub fn tmp(&self) -> PathBuf {
        self.tempdir.path().join("misc")
    }

    /// Get the scale or "test size". This is used to scale tests with more data. Defaults to `1`.
    pub fn scale(&self) -> usize {
        env::var("RAD_TEST_SCALE")
            .map(|s| {
                s.parse()
                    .expect("repository: invalid value for `RAD_TEST_SCALE`")
            })
            .unwrap_or(1)
    }

    /// Create a new node in this environment. This should be used when a running node
    /// is required. Use [`Environment::profile`] otherwise.
    pub fn node(&mut self, node: Config) -> Node<MemorySigner> {
        let alias = node.alias.clone();
        let profile = self.profile(profile::Config {
            node,
            ..Environment::config(alias)
        });
        Node::new(profile)
    }

    /// Create a new default configuration.
    pub fn config(alias: Alias) -> profile::Config {
        profile::Config {
            node: node::Config::test(alias),
            cli: cli::Config { hints: false },
            public_explorer: explorer::Explorer::default(),
            preferred_seeds: vec![],
            web: web::Config::default(),
        }
    }

    /// Create a new profile in this environment.
    /// This should be used when a running node is not required.
    pub fn profile(&mut self, config: profile::Config) -> Profile {
        let alias = config.alias().clone();
        let home = Home::new(
            self.tmp()
                .join("home")
                .join(alias.to_string())
                .join(".radicle"),
        )
        .unwrap();
        let keystore = Keystore::new(&home.keys());
        let keypair = KeyPair::from_seed(Seed::from([!(self.users as u8); 32]));
        let policies_db = home.node().join(POLICIES_DB_FILE);
        let cobs_db = home.cobs().join(COBS_DB_FILE);

        config.write(&home.config()).unwrap();

        let storage = Storage::open(
            home.storage(),
            git::UserInfo {
                alias,
                key: keypair.pk.into(),
            },
        )
        .unwrap();

        policy::Store::open(policies_db).unwrap();
        home.database_mut().unwrap(); // Just create the database.
        cob::cache::Store::open(cobs_db).unwrap();

        transport::local::register(storage.clone());
        keystore.store(keypair.clone(), "radicle", None).unwrap();

        // Ensures that each user has a unique but deterministic public key.
        self.users += 1;

        Profile {
            home,
            storage,
            keystore,
            public_key: keypair.pk.into(),
            config,
        }
    }
}

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
pub struct NodeHandle<G: Signer + cyphernet::Ecdh + 'static> {
    pub id: NodeId,
    pub storage: Storage,
    pub signer: G,
    pub home: Home,
    pub addr: net::SocketAddr,
    pub thread: ManuallyDrop<thread::JoinHandle<Result<(), runtime::Error>>>,
    pub handle: ManuallyDrop<Handle>,
}

impl<G: Signer + cyphernet::Ecdh + 'static> Drop for NodeHandle<G> {
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
        Database::reader(self.home.node().join(node::NODE_DB_FILE))
            .unwrap()
            .entries()
            .unwrap()
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
            .env("RAD_HOME", self.home.path().to_string_lossy().to_string())
            .env("RAD_PASSPHRASE", "radicle")
            .env("TZ", "UTC")
            .env("LANG", "C")
            .env(radicle::cob::git::RAD_COMMIT_TIME, "1671125284")
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
        let listen = vec![([0, 0, 0, 0], 0).into()];
        let proxy = net::SocketAddr::new(net::Ipv4Addr::LOCALHOST.into(), 9050);
        let (_, signals) = chan::bounded(1);
        let rt = Runtime::init(
            self.home.clone(),
            self.config,
            listen,
            proxy,
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
            name,
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
        for rid in node.storage.inventory().unwrap() {
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

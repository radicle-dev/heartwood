use std::io::BufRead as _;
use std::mem::ManuallyDrop;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::mpsc;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io, iter, net, process, thread, time,
    time::Duration,
};

use protocol::service;
use radicle::Storage;
use radicle::cob;
use radicle::cob::issue;
use radicle::crypto::{Signer as _, SigningKey};
use radicle::git;
use radicle::git::fmt::refname;
use radicle::identity::{RepoId, Visibility};
use radicle::node::Config;
use radicle::node::Event;
use radicle::node::NodeId;
use radicle::node::config::ConnectAddress;
use radicle::node::policy::store as policy;
use radicle::node::seed::Store as _;
use radicle::node::{self, Alias};
use radicle::node::{ConnectOptions, ConnectResult, Handle as _};
use radicle::node::{Database, POLICIES_DB_FILE};
use radicle::profile::{Home, Profile, env};
use radicle::rad;
use radicle::storage::git::transport;
use radicle::storage::{ReadStorage as _, RemoteRepository as _, SignRepository as _};
use radicle::test::fixtures;

use crate::runtime::{self, Runtime, handle::Handle};

/// A node that can be run.
pub struct Node {
    pub id: NodeId,
    pub home: Home,
    pub secret_key: SigningKey,
    pub storage: Storage,
    pub config: Config,
    pub db: service::Stores<Database>,
    pub policies: policy::Store<policy::Write>,
}

impl Node {
    pub fn new(profile: Profile) -> Self {
        let secret_key = profile.keystore.secret_key(None).unwrap().unwrap();
        let id = *profile.id();
        let policies_db = profile.home.node().join(POLICIES_DB_FILE);
        let policies = policy::Store::open(policies_db).unwrap();
        let db = profile.database_mut().unwrap();
        let db = service::Stores::from(db);

        Node {
            id,
            home: profile.home,
            config: profile.config.node,
            secret_key,
            db,
            policies,
            storage: profile.storage,
        }
    }
}

/// Handle to a running node.
pub struct NodeHandle {
    pub id: NodeId,
    pub alias: Alias,
    pub storage: Storage,
    pub signer: SigningKey,
    pub home: Home,
    pub addr: net::SocketAddr,
    pub thread: ManuallyDrop<thread::JoinHandle<Result<(), runtime::Error>>>,
    pub handle: ManuallyDrop<Handle>,
}

impl Drop for NodeHandle {
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

impl NodeHandle {
    /// Connect this node to another node, and wait for the connection to be established both ways.
    ///
    /// If the remote has blocked this node, return once the remote rejects the connection.
    pub fn connect(&mut self, remote: &NodeHandle) -> &mut Self {
        let events = remote.handle.events();
        let result = self
            .handle
            .connect(remote.id, remote.addr.into(), ConnectOptions::default())
            .unwrap();

        if matches!(result, ConnectResult::Connected) {
            let connected = remote
                .handle
                .sessions()
                .unwrap()
                .iter()
                .any(|session| session.nid == self.id && session.state.is_connected());

            if !connected {
                events
                    .wait(
                        |e| match e {
                            Event::PeerConnected { nid } | Event::PeerDisconnected { nid, .. }
                                if nid == &self.id =>
                            {
                                Some(())
                            }
                            _ => None,
                        },
                        Duration::from_secs(6),
                    )
                    .unwrap();
            }
        }

        self
    }

    pub fn disconnect(&mut self, remote: &mut NodeHandle) {
        self.handle.disconnect(remote.id).unwrap();
        remote.handle.disconnect(self.id).unwrap();
    }

    /// Shutdown node.
    pub fn shutdown(self) {
        drop(self)
    }

    /// Get the full address of this node.
    pub fn address(&self) -> ConnectAddress {
        ConnectAddress::new(self.id, node::Address::from(self.addr))
    }

    /// Get routing table entries.
    pub fn routing(&self) -> impl Iterator<Item = (RepoId, NodeId)> {
        use node::routing::Store as _;

        self.home
            .routing_mut(node::db::config::Config::default())
            .unwrap()
            .entries()
            .unwrap()
    }

    pub fn inventory(&self) -> impl Iterator<Item = RepoId> + '_ {
        self.routing()
            .filter(|(_, n)| *n == self.id)
            .map(|(r, _)| r)
    }

    /// Get sync status of a repo.
    pub fn synced_seeds(&self, rid: &RepoId) -> Vec<node::seed::SyncedSeed> {
        let db = Database::reader(
            self.home.node().join(node::NODE_DB_FILE),
            node::db::config::Config::default(),
        )
        .unwrap();
        let seeds = db.seeds_for(rid).unwrap();

        seeds.into_iter().collect::<Result<Vec<_>, _>>().unwrap()
    }

    /// Wait until this node's routing table matches the remotes.
    pub fn converge<'a>(
        &'a self,
        remotes: impl IntoIterator<Item = &'a NodeHandle>,
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
            let seeds = self.handle.seeds_for(*rid, [self.id]).unwrap();
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
            if let Ok(repo) = self.storage.repository(*rid)
                && repo.remote(nid).is_ok()
            {
                log::debug!(target: "test", "Node {} has {rid}/{nid}", self.id);
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

    /// Clone a repo into a directory.
    pub fn clone<P: AsRef<Path>>(&self, rid: RepoId, cwd: P) -> io::Result<()> {
        self.rad("clone", &[rid.to_string().as_str()], cwd)
    }

    /// Clone a repo and initialize our namespace by pushing its default branch.
    ///
    /// This function is called "fork" for historical reasons. There used to
    /// be a command `rad fork` that was used to initialize namespaces.
    pub fn fork<P: AsRef<Path>>(&self, rid: RepoId, cwd: P) -> io::Result<()> {
        let cwd = cwd.as_ref();
        self.clone(rid, cwd)?;

        let doc = self
            .storage
            .get(rid)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other(format!("repository {rid} was not found")))?;

        let project = doc.project().map_err(io::Error::other)?;
        let working =
            git::raw::Repository::open(cwd.join(project.name())).map_err(io::Error::other)?;
        let branch = git::fmt::Qualified::from(git::fmt::lit::refs_heads(project.default_branch()));

        transport::local::register(self.storage.clone());
        git::push(&working, &rad::REMOTE_NAME, [(&branch, &branch)]).map_err(io::Error::other)?;
        self.storage
            .repository(rid)
            .map_err(io::Error::other)?
            .sign_refs(&self.signer)
            .map_err(io::Error::other)?;
        self.announce(rid, 1, cwd)?;

        Ok(())
    }

    /// Announce a repo.
    pub fn announce<P: AsRef<Path>>(&self, rid: RepoId, replicas: usize, cwd: P) -> io::Result<()> {
        self.rad(
            "sync",
            &[
                "--repo",
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
            .env(env::RAD_RNG_SEED, "0")
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
    pub fn issue(&mut self, rid: RepoId, title: cob::Title, desc: &str) -> cob::ObjectId {
        let repo = self.storage.repository(rid).unwrap();
        let mut issues = issue::Cache::no_cache(&repo, &self.signer).unwrap();
        *issues.create(title, desc, &[], &[], []).unwrap().id()
    }

    /// Perform a commit to `refname`, within the node's namespace, by
    /// generating a blob of random data to a random path in a new tree.
    ///
    /// If the reference does not exist, a new one will be created with the new
    /// commit as its target.
    ///
    /// If the reference already exists, then its target is used as the parent
    /// of the new commit, and the reference will be updated.
    ///
    /// The `rad/sigrefs` are then updated to reflect the new change.
    pub fn commit_to(&self, rid: RepoId, refname: impl AsRef<git::fmt::RefStr>) {
        use radicle::test::arbitrary;

        let refname = match git::fmt::Qualified::from_refstr(refname.as_ref()) {
            None => git::fmt::lit::refs_heads(refname).into(),
            Some(refname) => refname,
        };
        let refname = refname.with_namespace(git::fmt::Component::from(&self.id));

        let repo = self.storage.repository(rid).unwrap();
        let raw = &repo.backend;

        let info = self.storage.info();
        let author = git::raw::Signature::now(&info.name(), &info.email()).unwrap();

        let tree = {
            let mut tb = raw.treebuilder(None).unwrap();
            let blob = raw.blob(&arbitrary::vec::<u8>(100)).unwrap();
            tb.insert(
                arbitrary::alphanumeric(10),
                blob,
                git::raw::FileMode::Blob.into(),
            )
            .unwrap();
            let oid = tb.write().unwrap();
            raw.find_tree(oid).unwrap()
        };
        let parent = {
            let target = raw
                .find_reference(refname.as_str())
                .ok()
                .and_then(|r| r.target());
            target.and_then(|oid| raw.find_commit(oid).ok())
        };
        match parent {
            None => repo
                .backend
                .commit(
                    Some(refname.as_str()),
                    &author,
                    &author,
                    "New commit",
                    &tree,
                    &[],
                )
                .unwrap(),
            Some(parent) => repo
                .backend
                .commit(
                    Some(refname.as_str()),
                    &author,
                    &author,
                    "New commit",
                    &tree,
                    &[&parent],
                )
                .unwrap(),
        };
        repo.sign_refs(&self.signer).unwrap();
    }
}

impl Node {
    /// Create a new node.
    pub fn init(base: &Path, config: Config, id: usize) -> Self {
        let home = base.join(config.alias.to_string());
        let home = Home::new(home).unwrap();
        let secret_key = SigningKey::mock(id);
        let nid = NodeId::from(*secret_key.public_key());
        let storage = Storage::open(
            home.storage(),
            git::UserInfo {
                alias: config.alias.clone(),
                key: nid,
            },
        )
        .unwrap();
        let policies = home.policies_mut().unwrap();
        let db = home
            .database_mut(node::db::config::Config::default())
            .unwrap();
        let db = service::Stores::from(db);

        log::debug!(target: "test", "Node::init {}: {}", config.alias, nid);
        Self {
            id: nid,
            home,
            secret_key,
            storage,
            config,
            db,
            policies,
        }
    }
}

impl Node {
    /// Spawn a node in its own thread.
    pub fn spawn(self) -> NodeHandle {
        let alias = self.config.alias.clone();

        let listen = vec![(Ipv4Addr::LOCALHOST, 0).into()];
        let (_, signals) = mpsc::sync_channel(1);
        let rt = Runtime::init(
            self.home.clone(),
            self.config,
            self.home.socket_default(),
            listen,
            signals,
            self.secret_key.clone(),
        )
        .unwrap();

        let id = NodeId::from(*self.secret_key.public_key());
        let handle = ManuallyDrop::new(rt.handle.clone());
        let thread = ManuallyDrop::new(runtime::thread::spawn(&id, "runtime", move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(rt.run())
        }));
        let addr = loop {
            if let Some(addr) = handle.listen_addrs().unwrap().into_iter().next() {
                break addr;
            }
            thread::sleep(Duration::from_millis(10));
        };

        NodeHandle {
            id,
            alias,
            storage: self.storage,
            signer: self.secret_key,
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
            &self.secret_key,
            &self.storage,
        )
        .map(|(id, _, _)| id)
        .unwrap();

        assert!(self.policies.seed(&id, node::policy::Scope::All).unwrap());

        log::debug!(
            target: "test",
            "Initialized project {id} for node {}", NodeId::from(*self.secret_key.public_key())
        );

        // Push local branches to storage.
        let mut refs = Vec::<(git::fmt::Qualified, git::fmt::Qualified)>::new();
        for branch in repo.branches(Some(git::raw::BranchType::Local)).unwrap() {
            let (branch, _) = branch.unwrap();
            let name = git::fmt::RefString::try_from(branch.name().unwrap().unwrap()).unwrap();

            refs.push((
                git::fmt::lit::refs_heads(&name).into(),
                git::fmt::lit::refs_heads(&name).into(),
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
            .sign_refs(&self.secret_key)
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
pub fn converge<'a>(nodes: impl IntoIterator<Item = &'a NodeHandle>) -> BTreeSet<(RepoId, NodeId)> {
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

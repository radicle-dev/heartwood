use std::path::PathBuf;
use std::str::FromStr;

use localtime::LocalTime;
use radicle::cob::cache::COBS_DB_FILE;
use radicle::crypto::ssh::{keystore::MemorySigner, Keystore};
use radicle::crypto::{KeyPair, Seed};
use radicle::node::policy::store as policy;
use radicle::node::{self, UserAgent};
use radicle::node::{Alias, Config, POLICIES_DB_FILE};
use radicle::profile::Home;
use radicle::profile::{self};
use radicle::storage::git::transport;
use radicle::{Profile, Storage};

use radicle_node::test::node::{Node, NodeHandle};

use crate::util::formula::formula;

pub(crate) mod config {
    use super::*;
    use radicle::node::config::{Config, Limits, Network, RateLimit, RateLimits};

    /// Configuration for a test seed node.
    ///
    /// It sets the `RateLimit::capacity` to `usize::MAX` ensuring
    /// that there are no rate limits for test nodes, since they all
    /// operate on the same IP address. This prevents any announcement
    /// messages from being dropped.
    pub fn seed(alias: &'static str) -> Config {
        Config {
            network: Network::Test,
            relay: node::config::Relay::Always,
            limits: Limits {
                rate: RateLimits {
                    inbound: RateLimit {
                        fill_rate: 1.0,
                        capacity: usize::MAX,
                    },
                    outbound: RateLimit {
                        fill_rate: 1.0,
                        capacity: usize::MAX,
                    },
                },
                ..Limits::default()
            },
            external_addresses: vec![node::Address::from_str(&format!(
                "{alias}.radicle.example:8776"
            ))
            .unwrap()],
            ..node(alias)
        }
    }

    /// Relay node config.
    pub fn relay(alias: &'static str) -> Config {
        Config {
            relay: node::config::Relay::Always,
            ..node(alias)
        }
    }

    /// Test node config.
    pub fn node(alias: &'static str) -> Config {
        Config::test(Alias::new(alias))
    }
}

/// Test environment.
pub struct Environment {
    tempdir: tempfile::TempDir,
    users: usize,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    /// Create a new test environment.
    fn named(name: &'static str) -> Self {
        Self {
            tempdir: tempfile::TempDir::with_prefix("radicle-".to_owned() + name).unwrap(),
            users: 0,
        }
    }

    /// Create a new test environment.
    pub fn new() -> Self {
        Self::named("")
    }

    /// Return the temp directory path.
    pub fn tempdir(&self) -> PathBuf {
        self.tempdir.path().into()
    }

    /// Path to the working directory designated for given alias.
    pub fn work(&self, has_alias: &impl HasAlias) -> PathBuf {
        self.tempdir().join("work").join(has_alias.alias().as_ref())
    }

    /// We don't have `RAD_HOME` or `HOME` to rely on to compute a home as usual.
    pub fn home(&self, alias: &Alias) -> Home {
        Home::new(
            self.tempdir()
                .join("home")
                .join(alias.to_string())
                .join(".radicle"),
        )
        .unwrap()
    }

    /// Create a new default configuration.
    pub fn config(&self, alias: &str) -> profile::Config {
        let alias = Alias::new(alias);
        profile::Config {
            node: node::Config::test(alias),
            cli: radicle::cli::Config { hints: false },
            public_explorer: radicle::explorer::Explorer::default(),
            preferred_seeds: vec![],
            web: radicle::web::Config::default(),
        }
    }

    /// Create a new profile in this environment.
    /// This should be used when a running node is not required.
    /// Using this function is only necessary if the desired configuration
    /// differs from the default provided by [`Environment::config`] as
    /// for this default the convenience function [`Environment::profile`]
    /// is provided.
    pub fn profile_with(&mut self, config: profile::Config) -> Profile {
        let alias = config.alias().clone();
        let home = self.home(&alias);
        let keypair = KeyPair::from_seed(Seed::from([!(self.users as u8); 32]));
        let policies_db = home.node().join(POLICIES_DB_FILE);
        let cobs_db = home.cobs().join(COBS_DB_FILE);

        config.write(&home.config()).unwrap();

        let storage = Storage::open(
            home.storage(),
            radicle::git::UserInfo {
                alias: alias.clone(),
                key: keypair.pk.into(),
            },
        )
        .unwrap();

        let mut db = home.cobs_db_mut().unwrap();
        db.migrate(radicle::cob::migrate::ignore).unwrap();

        policy::Store::open(policies_db).unwrap();
        home.database_mut()
            .unwrap()
            .init(
                &keypair.pk.into(),
                config.node.features(),
                &alias,
                &UserAgent::default(),
                LocalTime::now().into(),
                config.node.external_addresses.iter(),
            )
            .unwrap();

        radicle::cob::cache::Store::open(cobs_db).unwrap();

        transport::local::register(storage.clone());
        let keystore = Keystore::new(&home.keys());
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

    /// Create a new profile using a the default configuration from [`Environment::config`].
    pub fn profile(&mut self, alias: &'static str) -> Profile {
        self.profile_with(self.config(alias))
    }

    /// Create a new node in this environment. This should be used when a running node
    /// is required. Use [`Environment::profile`] otherwise.
    /// Using this function is only necessary when the node configuration differs
    /// from the standard ones ([`config::node`], [`config::relay`], [`config::seed`]),
    /// as for each of them a convenience function
    /// (resp. [`Environment::node`], [`Environment::relay`], [`Environment::seed`]).
    /// is provided to reduce boilerplate.
    pub fn node_with(&mut self, node: Config) -> Node<MemorySigner> {
        let alias = node.alias.clone();
        let profile = self.profile_with(profile::Config {
            node,
            ..self.config(alias.as_ref())
        });
        Node::new(profile)
    }

    /// Convenience method for creating a [`Node<MemorySigner>`]
    /// using configuration [`config::node`] within this [`Environment`].
    pub fn node(&mut self, alias: &'static str) -> Node<MemorySigner> {
        self.node_with(config::node(alias))
    }

    /// Convenience method for creating a [`Node<MemorySigner>`]
    /// using configuration [`config::relay`] within this [`Environment`].
    pub fn relay(&mut self, alias: &'static str) -> Node<MemorySigner> {
        self.node_with(config::relay(alias))
    }

    /// Convenience method for creating a [`Node<MemorySigner>`]
    /// using configuration [`config::seed`] within this [`Environment`].
    pub fn seed(&mut self, alias: &'static str) -> Node<MemorySigner> {
        self.node_with(config::seed(alias))
    }

    /// Convenience method for placing repository fixture.
    pub fn repository(
        &self,
        has_alias: &impl HasAlias,
    ) -> (radicle_cli::git::Repository, radicle_cli::git::Oid) {
        radicle::test::fixtures::repository(self.work(has_alias).as_path())
    }

    // Convenience method for exectuing a test formula with standard configuration.
    pub fn test(
        &self,
        test_file: &'static str,
        subject: &(impl HasAlias + HasHome),
    ) -> Result<(), Box<dyn std::error::Error>> {
        formula(
            self.work(subject).as_ref(),
            PathBuf::from("examples").join(test_file.to_owned() + ".md"),
        )?
        .env(
            "RAD_HOME",
            subject.home().path().to_path_buf().to_string_lossy(),
        )
        .run()?;

        Ok(())
    }

    pub fn tests(
        &self,
        test_files: impl IntoIterator<Item = &'static str>,
        subject: &(impl HasAlias + HasHome),
    ) -> Result<(), Box<dyn std::error::Error>> {
        for test_file in test_files {
            self.test(test_file, subject)?;
        }

        Ok(())
    }

    /// Convenience method for creating exactly one profile with alias "alice"
    /// and running tests within it.
    pub fn alice(test_files: impl IntoIterator<Item = &'static str>) {
        let mut env = Environment::new();
        let alice = env.profile("alice");
        env.repository(&alice);
        env.tests(test_files, &alice).unwrap();
    }
}

pub trait HasAlias {
    fn alias(&self) -> &Alias;
}

impl HasAlias for Node<MemorySigner> {
    fn alias(&self) -> &Alias {
        &self.config.alias
    }
}

impl HasAlias for Profile {
    fn alias(&self) -> &Alias {
        self.config.alias()
    }
}

impl<G> HasAlias for NodeHandle<G> {
    fn alias(&self) -> &Alias {
        &self.alias
    }
}

pub trait HasHome {
    fn home(&self) -> &Home;
}

impl HasHome for Profile {
    fn home(&self) -> &Home {
        &self.home
    }
}

impl HasHome for Node<MemorySigner> {
    fn home(&self) -> &Home {
        &self.home
    }
}

impl HasHome for NodeHandle<MemorySigner> {
    fn home(&self) -> &Home {
        &self.home
    }
}

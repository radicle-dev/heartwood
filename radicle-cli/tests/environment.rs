use std::path::PathBuf;
use std::str::FromStr;

use radicle::cob::cache::COBS_DB_FILE;
use radicle::crypto::ssh::{keystore::MemorySigner, Keystore};
use radicle::crypto::{KeyPair, Seed};
use radicle::node;
use radicle::node::policy::store as policy;
use radicle::node::{Alias, Config, POLICIES_DB_FILE};
use radicle::profile;
use radicle::profile::Home;
use radicle::storage::git::transport;
use radicle::{Profile, Storage};

use radicle_node::test::node::Node;

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

    /// We don't have `RAD_HOME` or `HOME` to rely on to compute a home as usual.
    pub fn home(&self, alias: &Alias) -> Home {
        Home::new(
            self.tmp()
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
                alias,
                key: keypair.pk.into(),
            },
        )
        .unwrap();

        policy::Store::open(policies_db).unwrap();
        home.database_mut().unwrap(); // Just create the database.
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
}

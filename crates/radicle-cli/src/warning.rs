use std::collections::HashMap;
use std::sync::LazyLock;

use radicle::node::config::ConnectAddress;
use radicle::node::policy::Scope;
use radicle::node::Address;
use radicle::profile::Config;

static NODES_RENAMED: LazyLock<HashMap<Address, Address>> = LazyLock::new(|| {
    HashMap::from([
        (
            "seed.radicle.garden:8776".parse().unwrap(),
            "iris.radicle.xyz:8776".parse().unwrap(),
        ),
        (
            "ash.radicle.garden:8776".parse().unwrap(),
            "rosa.radicle.xyz:8776".parse().unwrap(),
        ),
    ])
});

fn nodes_renamed_for_option(
    option: &'static str,
    iter: impl IntoIterator<Item = ConnectAddress>,
) -> Vec<String> {
    iter.into_iter().enumerate().fold(Vec::new(), |mut warnings, (i, value)| {
        let old: Address = value.into();
        if let Some(new) = NODES_RENAMED.get(&old) {
            warnings.push(format!(
                "Value of configuration option `{option}` at index {i} mentions node with address '{old}', which has been renamed to '{new}'. Please update your configuration."
            ));
        }
        warnings
    })
}

fn nodes_renamed(config: &Config) -> Vec<String> {
    let mut warnings = nodes_renamed_for_option("node.connect", config.node.connect.clone());
    warnings.extend(nodes_renamed_for_option(
        "preferredSeeds",
        config.preferred_seeds.clone(),
    ));

    warnings
}

fn implicit_seeding_policy_allow_scope(config: &Config) -> Vec<String> {
    use radicle::node::config::{DefaultSeedingPolicy, MigratingScope};

    if let DefaultSeedingPolicy::Allow {
        scope: MigratingScope::Implicit(scope),
    } = config.node.seeding_policy
    {
        vec![format!(
                "node 'seedingPolicy.scope' has been set to '{scope}' by default. This default value will be removed in a future release. Please explicitly set it to one of ['{}', '{}'] in your node config.",
                Scope::All,
                Scope::Followed,
        )]
    } else {
        vec![]
    }
}

pub(crate) fn config_warnings(config: &Config) -> Vec<String> {
    let mut warnings = nodes_renamed(config);
    warnings.extend(implicit_seeding_policy_allow_scope(config));

    warnings
}

/// Prints a deprecation warning to standard error.
pub(crate) fn deprecated(old: impl std::fmt::Display, new: impl std::fmt::Display) {
    eprintln!(
        "{} {} The command/option `{old}` is deprecated and will be removed. Please use `{new}` instead.",
        radicle_term::PREFIX_WARNING,
        radicle_term::Paint::yellow("Deprecated:").bold(),
    );
}

/// Prints an obsoletion warning to standard error.
pub(crate) fn obsolete(command: impl std::fmt::Display) {
    eprintln!(
        "{} {} The command `{command}` is obsolete and will be removed. Please stop using it.",
        radicle_term::PREFIX_WARNING,
        radicle_term::Paint::yellow("Obsolete:").bold(),
    );
}

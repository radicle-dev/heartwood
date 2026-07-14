use std::collections::HashMap;
use std::sync::LazyLock;

use radicle::node::config::ConnectAddress;
use radicle::node::{Address, Host};
use radicle::profile::Config;

const IRIS: &str = "iris.radicle.network";
const ROSA: &str = "rosa.radicle.network";

static NODES_RENAMED: LazyLock<HashMap<Host, Host>> = LazyLock::new(|| {
    HashMap::from([
        (
            Host::Dns("seed.radicle.garden".to_string()),
            Host::Dns(IRIS.to_string()),
        ),
        (
            Host::Dns("iris.radicle.xyz".to_string()),
            Host::Dns(IRIS.to_string()),
        ),
        (
            Host::Dns("ash.radicle.garden".to_string()),
            Host::Dns(ROSA.to_string()),
        ),
        (
            Host::Dns("rosa.radicle.xyz".to_string()),
            Host::Dns(ROSA.to_string()),
        ),
    ])
});

fn nodes_renamed_for_option(
    option: &'static str,
    iter: impl IntoIterator<Item = ConnectAddress>,
) -> Vec<String> {
    iter.into_iter().enumerate().fold(Vec::new(), |mut warnings, (i, value)| {
        let old = value.addr();
        let old = old.host();
        if let Some(new) = NODES_RENAMED.get(old) {
            warnings.push(format!(
                "Value of configuration option `{option}` at index {i} mentions node with hostname '{old}', which has been renamed to '{new}'. Please edit your configuration file to use the new address."
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
    use radicle::node::config::DefaultSeedingPolicy;
    use radicle::node::policy::Scope::*;

    let DefaultSeedingPolicy::Allow { scope } = config.node.seeding_policy else {
        return vec![];
    };

    if !scope.is_implicit() {
        return vec![];
    }

    vec![format!(
        "Configuration option 'node.seedingPolicy.scope' is not set, and thus takes the value '{}' by default. The default value will change to '{}' in a future release. Please edit your configuration file, and set it to one of ['{}', '{}'] explicitly.",
        scope.into_inner(),
        Followed,
        All,
        Followed,
    )]
}

fn ipv6_without_square_brackets(config: &Config) -> Vec<String> {
    fn zip(
        option: &'static str,
        iter: impl Iterator<Item = Address>,
    ) -> impl Iterator<Item = (&'static str, (usize, Address))> {
        std::iter::zip(
            std::iter::repeat(option),
            iter.enumerate().filter_map(|(i, address)| {
                #[allow(deprecated)]
                address
                    .is_ipv6_without_square_brackets()
                    .then_some((i, address))
            }),
        )
    }

    fn pick_addr<'a>(
        iter: impl Iterator<Item = &'a ConnectAddress>,
    ) -> impl Iterator<Item = Address> {
        iter.map(|connect_address| connect_address.addr().clone())
    }

    let chained = zip("preferredSeeds", pick_addr(config.preferred_seeds.iter()))
        .chain(zip("node.connect", pick_addr(config.node.connect.iter())))
        .chain(zip(
            "node.externalAddresses",
            config.node.external_addresses.iter().cloned(),
        ));

    chained.map(|(option, (i, address))|
        format!(
            "Value of configuration option `{option}` at zero-based index {i} mentions IPv6 address '{}' without square brackets. The address format will change, and this address will be rejected in the future. Please edit your configuration file to enclose the IPv6 address in square brackets. Combined with port information it should read '[{}]:{}'. Refer to RFC 5926, Sec. 6 as well as RFC 3986, Sec. D.1. and RFC 2732, Sec. 2.",
            address.host(),
            address.host(),
            address.port(),
        )
    ).collect()
}

pub(crate) fn config_warnings(config: &Config) -> Vec<String> {
    let mut warnings = nodes_renamed(config);
    warnings.extend(implicit_seeding_policy_allow_scope(config));
    warnings.extend(ipv6_without_square_brackets(config));

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
#[allow(dead_code)] // There currently is no obsolete command, but we keep this function for future use.
pub(crate) fn obsolete(command: impl std::fmt::Display) {
    eprintln!(
        "{} {} The command `{command}` is obsolete and will be removed. Please stop using it.",
        radicle_term::PREFIX_WARNING,
        radicle_term::Paint::yellow("Obsolete:").bold(),
    );
}

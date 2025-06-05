use radicle::node::Address;
use radicle::profile::Config;

use once_cell::sync::Lazy;

struct NodeRename {
    old: Address,
    new: Address,
}

static NODES_RENAMED: Lazy<[NodeRename; 2]> = Lazy::new(|| {
    [
        NodeRename {
            old: "seed.radicle.garden:8776".parse().unwrap(),
            new: "iris.radicle.xyz:8776".parse().unwrap(),
        },
        NodeRename {
            old: "ash.radicle.garden:8776".parse().unwrap(),
            new: "rosa.radicle.xyz:8776".parse().unwrap(),
        },
    ]
});

fn nodes_renamed_for_option<T>(
    option: &'static str,
    iter: impl IntoIterator<Item = T>,
) -> Vec<String>
where
    T: Into<Address> + Clone,
{
    let mut warnings: Vec<String> = vec![];

    for (i, value) in iter.into_iter().enumerate() {
        for rename in NODES_RENAMED.iter() {
            if value.clone().into() == rename.old {
                warnings.push(format!(
                    "Value of configuration option `{option}` at index {i} mentions node with address '{}', which has been renamed to '{}'. Please update your configuration.",
                    rename.old, rename.new
                ));
            }
        }
    }

    warnings
}

pub(crate) fn nodes_renamed(config: &Config) -> Vec<String> {
    let mut warnings = nodes_renamed_for_option("node.connect", config.node.connect.clone());
    warnings.extend(nodes_renamed_for_option(
        "preferred_seeds",
        config.preferred_seeds.clone(),
    ));
    warnings
}

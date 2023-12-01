use radicle::crypto::PublicKey;
use radicle::node::{policy, AliasStore};
use radicle::prelude::Did;
use radicle::Profile;

use crate::terminal as term;
use term::Element;

pub fn seeding(profile: &Profile) -> anyhow::Result<()> {
    let store = profile.policies()?;
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("RID")),
        term::format::default(String::from("Scope")),
        term::format::default(String::from("Policy")),
    ]);
    t.divider();

    for policy::Repo { id, scope, policy } in store.seed_policies()? {
        let id = id.to_string();
        let scope = scope.to_string();
        let policy = policy.to_string();

        t.push([
            term::format::highlight(id),
            term::format::secondary(scope),
            term::format::secondary(policy),
        ])
    }
    t.print();

    Ok(())
}

pub fn following(profile: &Profile) -> anyhow::Result<()> {
    let store = profile.policies()?;
    let aliases = profile.aliases();
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("DID")),
        term::format::default(String::from("Alias")),
        term::format::default(String::from("Policy")),
    ]);
    t.divider();

    for policy::Node { id, alias, policy } in store.follow_policies()? {
        t.push([
            term::format::highlight(Did::from(id).to_string()),
            match alias {
                None => term::format::secondary(fallback_alias(&id, &aliases)),
                Some(alias) => term::format::secondary(alias.to_string()),
            },
            term::format::secondary(policy.to_string()),
        ]);
    }
    t.print();

    Ok(())
}

fn fallback_alias(nid: &PublicKey, aliases: &impl AliasStore) -> String {
    aliases
        .alias(nid)
        .map_or("n/a".to_string(), |alias| alias.to_string())
}

use radicle::node::policy;
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

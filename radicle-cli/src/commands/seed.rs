use std::collections::BTreeSet;
use std::ffi::OsString;
use std::time;

use anyhow::anyhow;

use radicle::node::policy;
use radicle::node::policy::{Policy, Scope};
use radicle::node::Handle;
use radicle::{prelude::*, storage, Node};
use radicle_term::Element as _;

use crate::commands::rad_sync as sync;
use crate::node::SyncSettings;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::{self as term, Context as _};

pub const HELP: Help = Help {
    name: "seed",
    description: "Manage repository seeding policies",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad seed [<rid>] [--[no-]fetch] [--from <nid>] [--scope <scope>] [<option>...]

    The `seed` command, when no Repository ID (<rid>) is provided, will list the
    repositories being seeded.

    When a Repository ID (<rid>) is provided it updates or creates the seeding policy for
    that repository. To delete a seeding policy, use the `rad unseed` command.

    When seeding a repository, a scope can be specified: this can be either `all` or
    `followed`. When using `all`, all remote nodes will be followed for that repository.
    On the other hand, with `followed`, only the repository delegates will be followed,
    plus any remote that is explicitly followed via `rad follow <nid>`.

Options

    --[no-]fetch           Fetch repository after updating seeding policy
    --from <nid>           Fetch from the given node (may be specified multiple times)
    --timeout <secs>       Fetch timeout in seconds (default: 9)
    --scope <scope>        Peer follow scope for this repository
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub enum Operation {
    Seed {
        rid: RepoId,
        fetch: bool,
        seeds: BTreeSet<NodeId>,
        timeout: time::Duration,
        scope: Scope,
    },
    List,
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid: Option<RepoId> = None;
        let mut scope: Option<Scope> = None;
        let mut fetch: Option<bool> = None;
        let mut timeout = time::Duration::from_secs(9);
        let mut seeds: BTreeSet<NodeId> = BTreeSet::new();
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match &arg {
                Value(val) => {
                    rid = Some(term::args::rid(val)?);
                }
                Long("scope") => {
                    let val = parser.value()?;
                    scope = Some(term::args::parse_value("scope", val)?);
                }
                Long("fetch") => {
                    fetch = Some(true);
                }
                Long("no-fetch") => {
                    fetch = Some(false);
                }
                Long("from") => {
                    let val = parser.value()?;
                    let nid = term::args::nid(&val)?;

                    seeds.insert(nid);
                }
                Long("timeout") | Short('t') => {
                    let value = parser.value()?;
                    let secs = term::args::parse_value("timeout", value)?;

                    timeout = time::Duration::from_secs(secs);
                }
                Long("verbose") | Short('v') => verbose = true,
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match rid {
            Some(rid) => Operation::Seed {
                rid,
                fetch: fetch.unwrap_or(true),
                scope: scope.unwrap_or(Scope::All),
                timeout,
                seeds,
            },
            None => Operation::List,
        };

        Ok((Options { op, verbose }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    match options.op {
        Operation::Seed {
            rid,
            fetch,
            scope,
            timeout,
            seeds,
        } => {
            update(rid, scope, &mut node, &profile)?;

            if fetch && node.is_running() {
                sync::fetch(
                    rid,
                    SyncSettings::default()
                        .seeds(seeds)
                        .timeout(timeout)
                        .with_profile(&profile),
                    &mut node,
                    &profile,
                )?;
            }
        }
        Operation::List => seeding(&profile)?,
    }

    Ok(())
}

pub fn update(
    rid: RepoId,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    let updated = profile.seed(rid, scope, node)?;
    let outcome = if updated { "updated" } else { "exists" };

    if let Ok(repo) = profile.storage.repository(rid) {
        if repo.identity_doc()?.is_public() {
            profile.add_inventory(rid, node)?;
            term::success!(
                term,
                "Inventory updated with {}",
                term::format::tertiary(rid)
            );
        }
    }

    term::success!(
        term,
        "Seeding policy {outcome} for {} with scope '{scope}'",
        term::format::tertiary(rid),
    );

    Ok(())
}

pub fn seeding(profile: &Profile) -> anyhow::Result<()> {
    let term = profile.terminal();
    let store = profile.policies()?;
    let storage = &profile.storage;
    let mut t = term::Table::new(term::table::TableOptions::bordered());

    t.header([
        term::format::default(String::from("Repository")),
        term::format::default(String::from("Name")),
        term::format::default(String::from("Policy")),
        term::format::default(String::from("Scope")),
    ]);
    t.divider();

    for policy::SeedPolicy { rid, policy } in store.seed_policies()? {
        let id = rid.to_string();
        let name = storage
            .repository(rid)
            .map_err(storage::RepositoryError::from)
            .and_then(|repo| repo.project().map(|proj| proj.name().to_string()))
            .unwrap_or_default();
        let scope = policy.scope().unwrap_or_default().to_string();
        let policy = term::format::policy(&Policy::from(policy));

        t.push([
            term::format::tertiary(id),
            name.into(),
            policy,
            term::format::dim(scope),
        ])
    }

    if t.is_empty() {
        term.println(term::format::dim("No seeding policies to show."));
    } else {
        t.print_to(&term);
    }

    Ok(())
}

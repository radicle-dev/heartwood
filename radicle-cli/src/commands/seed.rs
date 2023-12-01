use std::ffi::OsString;
use std::time;

use anyhow::anyhow;

use radicle::node::policy::Scope;
use radicle::node::Handle;
use radicle::{prelude::*, Node};

use crate::commands::rad_sync as sync;
use crate::terminal::args::{Args, Error, Help};
use crate::{project, terminal as term};

pub const HELP: Help = Help {
    name: "seed",
    description: "Manage repository seeding policies",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad seed <rid> [-d | --delete] [--[no-]fetch] [--scope <scope>] [<option>...]

    The `seed` command takes a Repository ID (<rid>) and updates the seeding policy
    for that repository. By default, a seeding policy will be created or updated.
    To delete a policy, use the `--delete` flag.

    When seeding a repository, a scope can be specified: this can be either `all` or
    `followed`. When using `all`, all remote nodes will be followed for that repository.
    On the other hand, with `followed`, only the repository delegates will be followed,
    plus any remote that is explicitly followed via `rad follow <nid>`.

Options

    --delete, -d           Delete the seeding policy
    --[no-]fetch           Fetch repository after updating seeding policy
    --scope <scope>        Peer follow scope for this repository
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub rid: Id,
    pub scope: Scope,
    pub delete: bool,
    pub fetch: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid: Option<Id> = None;
        let mut scope: Option<Scope> = None;
        let mut fetch = true;
        let mut delete = false;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match &arg {
                Value(val) => {
                    rid = Some(term::args::rid(val)?);
                }
                Long("scope") if scope.is_none() => {
                    let val = parser.value()?;
                    scope = Some(term::args::parse_value("scope", val)?);
                }
                Long("delete") | Short('d') => delete = true,
                Long("fetch") => fetch = true,
                Long("no-fetch") => fetch = false,
                Long("verbose") | Short('v') => verbose = true,
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        if scope.is_some() && delete {
            anyhow::bail!("`--scope` may not be used with `--delete` or `-d`");
        }
        if fetch && delete {
            anyhow::bail!("`--fetch` may not be used with `--delete` or `-d`");
        }

        Ok((
            Options {
                rid: rid.ok_or_else(|| anyhow!("a Repository ID must be specified"))?,
                scope: scope.unwrap_or(Scope::All),
                delete,
                fetch,
                verbose,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());
    let rid = options.rid;
    let scope = options.scope;

    if options.delete {
        delete(rid, &mut node, &profile)?;
    } else {
        update(rid, scope, &mut node, &profile)?;

        if options.fetch && node.is_running() {
            sync::fetch(
                rid,
                sync::RepoSync::default(),
                time::Duration::from_secs(6),
                &mut node,
            )?;
        }
    }

    Ok(())
}

pub fn update(
    rid: Id,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    let updated = project::track(rid, scope, node, profile)?;
    let outcome = if updated { "updated" } else { "exists" };

    term::success!(
        "Seeding policy {outcome} for {} with scope '{scope}'",
        term::format::tertiary(rid),
    );

    Ok(())
}

pub fn delete(rid: Id, node: &mut Node, profile: &Profile) -> anyhow::Result<()> {
    if project::untrack(rid, node, profile)? {
        term::success!("Seeding policy for {} removed", term::format::tertiary(rid));
    }
    Ok(())
}

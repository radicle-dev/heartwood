use std::ffi::OsString;
use std::path::Path;

use localtime::LocalDuration;
use localtime::LocalTime;
use radicle::cob::issue;
use radicle::cob::patch;
use radicle::git;
use radicle::node::address;
use radicle::node::routing;
use radicle::storage::{ReadRepository, ReadStorage, WriteRepository};
use radicle_term::Element;
use serde::Serialize;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "stats",
    description: "Displays aggregated repository and node metrics",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad stats [<option>...]

Options

    --help       Print help
"#,
};

#[derive(Default, Serialize)]
struct NodeStats {
    all: usize,
    public: usize,
    online: usize,
    seeding: usize,
}

#[derive(Default, Serialize)]
struct LocalStats {
    repos: usize,
    issues: usize,
    patches: usize,
    pushes: usize,
    forks: usize,
}

#[derive(Default, Serialize)]
struct RepoStats {
    unique: usize,
    replicas: usize,
}

#[derive(Default, Serialize)]
struct Stats {
    local: LocalStats,
    repos: RepoStats,
    nodes: NodeStats,
}

#[derive(Default, Debug, Eq, PartialEq)]
pub struct Options {}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);

        #[allow(clippy::never_loop)]
        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((Options {}, vec![]))
    }
}

pub fn run(_options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let mut stats = Stats::default();

    for repo in storage.repositories()? {
        let repo = storage.repository(repo.rid)?;
        let issues = issue::Issues::open(&repo)?.counts()?;
        let patches = patch::Patches::open(&repo)?.counts()?;

        stats.local.issues += issues.total();
        stats.local.patches += patches.total();
        stats.local.repos += 1;

        for remote in repo.remote_ids()? {
            let remote = remote?;
            let sigrefs = repo.reference_oid(&remote, &git::refs::storage::SIGREFS_BRANCH)?;
            let mut walk = repo.raw().revwalk()?;
            walk.push(*sigrefs)?;

            stats.local.pushes += walk.count();
            stats.local.forks += 1;
        }
    }

    let db = profile.database()?;
    stats.nodes.all = address::Store::nodes(&db)?;
    stats.repos.replicas = routing::Store::len(&db)?;

    {
        let row = db
            .db
            .prepare("SELECT COUNT(DISTINCT repo) FROM routing")?
            // SAFETY: `COUNT` always returns a row.
            .into_iter()
            .next()
            .unwrap()?;
        let count = row.read::<i64, _>(0) as usize;

        stats.repos.unique = count;
    }

    {
        let now = LocalTime::now();
        let since = now - LocalDuration::from_mins(60 * 24); // 1 day.
        let mut stmt = db.db.prepare(
            "SELECT COUNT(DISTINCT node) FROM announcements WHERE timestamp >= ?1 and timestamp < ?2",
        )?;

        stmt.bind((1, since.as_millis() as i64))?;
        stmt.bind((2, now.as_millis() as i64))?;

        // SAFETY: `COUNT` always returns a row.
        let row = stmt.into_iter().next().unwrap()?;
        let count = row.read::<i64, _>(0) as usize;

        stats.nodes.online = count;
    }

    {
        let row = db
            .db
            .prepare("SELECT COUNT(DISTINCT node) FROM addresses")?
            .into_iter()
            .next()
            // SAFETY: `COUNT` always returns a row.
            .unwrap()?;
        let count = row.read::<i64, _>(0) as usize;

        stats.nodes.public = count;
    }

    {
        let row = db
            .db
            .prepare("SELECT COUNT(DISTINCT node) FROM routing")?
            .into_iter()
            .next()
            // SAFETY: `COUNT` always returns a row.
            .unwrap()?;
        let count = row.read::<i64, _>(0) as usize;

        stats.nodes.seeding = count;
    }

    let output = term::json::to_pretty(&stats, Path::new("stats.json"))?;
    output.print();

    Ok(())
}

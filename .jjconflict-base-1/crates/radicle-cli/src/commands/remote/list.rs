use std::collections::HashSet;

use radicle::Profile;
use radicle::git::Url;
use radicle::identity::{Did, RepoId};
use radicle::node::{Alias, AliasStore as _, NodeId};
use radicle::storage::ReadStorage as _;
use radicle_term::{Element, Table};

use crate::git;
use crate::terminal as term;

#[derive(Debug)]
pub enum Direction {
    Push(Url),
    Fetch(Url),
}

#[derive(Debug)]
pub struct Tracked {
    name: String,
    direction: Option<Direction>,
}

impl Tracked {
    fn namespace(&self) -> Option<NodeId> {
        match self.direction.as_ref()? {
            Direction::Push(url) => url.namespace,
            Direction::Fetch(url) => url.namespace,
        }
    }
}

#[derive(Debug)]
pub struct Untracked {
    remote: NodeId,
    alias: Option<Alias>,
}

pub fn tracked(working: &git::Repository) -> anyhow::Result<Vec<Tracked>> {
    Ok(git::rad_remotes(working)?
        .into_iter()
        .flat_map(|remote| {
            [
                Tracked {
                    name: remote.name.clone(),
                    direction: Some(Direction::Fetch(remote.url)),
                },
                Tracked {
                    name: remote.name,
                    direction: remote.pushurl.map(Direction::Push),
                },
            ]
        })
        .collect())
}

pub fn untracked<'a>(
    rid: RepoId,
    profile: &Profile,
    tracked: impl Iterator<Item = &'a Tracked>,
) -> anyhow::Result<Vec<Untracked>> {
    let repo = profile.storage.repository(rid)?;
    let aliases = profile.aliases();
    let remotes = repo.remote_ids()?;
    let git_remotes = tracked
        .filter_map(|tracked| tracked.namespace())
        .collect::<HashSet<_>>();
    Ok(remotes
        .filter_map(|remote| {
            remote
                .map(|remote| {
                    (!git_remotes.contains(&remote)).then_some(Untracked {
                        remote,
                        alias: aliases.alias(&remote),
                    })
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn print_tracked<'a>(tracked: impl Iterator<Item = &'a Tracked>) {
    Table::from_iter(tracked.into_iter().flat_map(|Tracked { direction, name }| {
        let Some(direction) = direction else {
            return None;
        };

        let (dir, url) = match direction {
            Direction::Push(url) => ("push", url),
            Direction::Fetch(url) => ("fetch", url),
        };
        let description = url.namespace.map_or(
            term::format::dim("(canonical upstream)".to_string()).italic(),
            |namespace| term::format::tertiary(namespace.to_string()),
        );
        Some([
            term::format::bold(name.clone()),
            description,
            term::format::parens(term::format::secondary(dir.to_owned())),
        ])
    }))
    .print();
}

pub fn print_untracked<'a>(untracked: impl Iterator<Item = &'a Untracked>) {
    Table::from_iter(untracked.into_iter().map(|Untracked { remote, alias }| {
        [
            match alias {
                None => term::format::secondary("n/a".to_string()),
                Some(alias) => term::format::secondary(alias.to_string()),
            },
            term::format::highlight(Did::from(remote).to_string()),
        ]
    }))
    .print();
}

pub mod error;

use either::Either;
use radicle::git::raw::ErrorExt as _;
use radicle::git::{
    self, Oid,
    fmt::{Namespaced, Qualified},
};
use radicle::storage::git::Repository;

use super::refs::{Applied, Policy, RefUpdate, Update};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ancestry {
    Equal,
    Ahead,
    Behind,
    Diverged,
}

pub enum Updated<'a> {
    Accepted(RefUpdate),
    Rejected(Update<'a>),
}

impl From<RefUpdate> for Updated<'_> {
    fn from(up: RefUpdate) -> Self {
        Updated::Accepted(up)
    }
}

impl<'a> From<Update<'a>> for Updated<'a> {
    fn from(up: Update<'a>) -> Self {
        Updated::Rejected(up)
    }
}

pub fn contains(repo: &Repository, oid: Oid) -> Result<bool, error::Contains> {
    repo.backend
        .odb()
        .map(|odb| odb.exists(oid.into()))
        .map_err(error::Contains)
}

/// Find the object identified by `oid` and peel it to its associated
/// commit `Oid`.
///
/// # Errors
///
/// - The object was not found
/// - The object does not peel to a commit
/// - Attempting to find the object fails
fn find_and_peel(repo: &Repository, oid: Oid) -> Result<Oid, error::Ancestry> {
    match repo.backend.find_object(oid.into(), None) {
        Ok(object) => Ok(object
            .peel(git::raw::ObjectType::Commit)
            .map_err(|err| error::Ancestry::Peel { oid, err })?
            .id()
            .into()),
        Err(e) if e.is_not_found() => Err(error::Ancestry::Missing { oid }),
        Err(err) => Err(error::Ancestry::Object { oid, err }),
    }
}

/// Peels the two objects to commits (see [`find_and_peel`]) and determines
/// their ancestry relationship (see [`ahead_behind`]).
pub fn ancestry(repo: &Repository, old: Oid, new: Oid) -> Result<Ancestry, error::Ancestry> {
    let old = find_and_peel(repo, old)?;
    let new = find_and_peel(repo, new)?;

    ahead_behind(repo, old, new)
}

/// Determine the ancestry relationship between two commits.
pub fn ahead_behind(
    repo: &Repository,
    old_commit: Oid,
    new_commit: Oid,
) -> Result<Ancestry, error::Ancestry> {
    if old_commit == new_commit {
        return Ok(Ancestry::Equal);
    }

    let (ahead, behind) = repo
        .backend
        .graph_ahead_behind(new_commit.into(), old_commit.into())
        .map_err(|err| error::Ancestry::Check {
            old: old_commit,
            new: new_commit,
            err,
        })?;

    if ahead > 0 && behind == 0 {
        Ok(Ancestry::Ahead)
    } else if ahead == 0 && behind > 0 {
        Ok(Ancestry::Behind)
    } else {
        Ok(Ancestry::Diverged)
    }
}

pub fn refname_to_id<'a, N>(repo: &Repository, refname: N) -> Result<Option<Oid>, error::Resolve>
where
    N: Into<Qualified<'a>>,
{
    use git::raw::ErrorCode::NotFound;

    let refname = refname.into();
    match repo.backend.refname_to_id(refname.as_ref()) {
        Ok(oid) => Ok(Some(oid.into())),
        Err(e) if matches!(e.code(), NotFound) => Ok(None),
        Err(err) => Err(error::Resolve {
            name: refname.to_owned(),
            err,
        }),
    }
}

pub fn update<'a, I>(repo: &Repository, updates: I) -> Result<Applied<'a>, error::Update>
where
    I: IntoIterator<Item = Update<'a>>,
{
    let mut applied = Applied::default();
    for up in updates.into_iter() {
        match up {
            Update::Direct {
                name,
                target,
                no_ff,
            } => match direct(repo, name, target, no_ff)? {
                Updated::Rejected(r) => applied.rejected.push(r),
                Updated::Accepted(u) => applied.updated.push(u),
            },
            Update::Prune { name, prev } => match prune(repo, name, prev)? {
                Updated::Rejected(r) => applied.rejected.push(r),
                Updated::Accepted(u) => applied.updated.push(u),
            },
        }
    }

    Ok(applied)
}

fn direct<'a>(
    repo: &Repository,
    name: Namespaced<'a>,
    target: Oid,
    no_ff: Policy,
) -> Result<Updated<'a>, error::Update> {
    let Some(reference) = find(repo, &name)? else {
        repo.backend
            .reference(name.as_ref(), target.into(), false, "radicle: create")
            .map_err(|err| error::Update::Create {
                name: name.to_owned(),
                target,
                err,
            })?;

        return Ok(RefUpdate::Created {
            name: name.to_ref_string(),
            oid: target,
        }
        .into());
    };

    let Some(prev) = reference.target() else {
        // This should never happen, as there are no facilities to create
        // symbolic references in Radicle namespaces. If it does, e.g. because
        // some external program or the user themselves created it, we better
        // do not touch it.
        return Err(error::Update::Symbolic {
            name: name.to_owned(),
        });
    };

    if target == prev {
        // If the two objects are identical, their ancestry does not matter,
        // we can always skip the update.
        return Ok(RefUpdate::Skipped {
            name: name.to_ref_string(),
            oid: target,
        }
        .into());
    }

    let ancestry = {
        use git::raw::ObjectType::{self, *};
        const ANY_KIND: Option<ObjectType> = Some(Any);

        let prev = repo.backend.find_object(prev, ANY_KIND).map_err(|err| {
            error::Update::Ancestry(error::Ancestry::Object {
                oid: prev.into(),
                err,
            })
        })?;

        let target = repo
            .backend
            .find_object(target.into(), ANY_KIND)
            .map_err(|err| error::Update::Ancestry(error::Ancestry::Object { oid: target, err }))?;

        match (prev.kind(), target.kind()) {
            (Some(Commit), Some(Commit)) => {
                // This is the common case, we have two commits to compare.
                let prev = prev.id().into();
                let target = target.id().into();
                Some(ahead_behind(repo, prev, target)?)
            }
            (Some(Tag), Some(Tag)) => {
                // Even though these tags might point to the same commit,
                // refuse to peel, because that tag itself has changed
                // (e.g. its name or signature).
                None
            }
            (Some(Commit | Tag), Some(Commit | Tag)) => {
                // The reference changes from a commit to a tag or vice versa.
                None
            }
            _ => {
                // One of the objects is not a commit or a tag, we're clueless.
                None
            }
        }
    };

    match ancestry {
        Some(Ancestry::Equal) => Ok(RefUpdate::Skipped {
            name: name.to_ref_string(),
            oid: target,
        }
        .into()),
        Some(Ancestry::Ahead) => {
            // N.b. the update is a fast-forward so we can safely
            // pass `force: true`.
            repo.backend
                .reference(name.as_ref(), target.into(), true, "radicle: update")
                .map_err(|err| error::Update::Create {
                    name: name.to_owned(),
                    target,
                    err,
                })?;
            Ok(RefUpdate::from(name.to_ref_string(), prev, target).into())
        }
        Some(Ancestry::Behind | Ancestry::Diverged) | None if matches!(no_ff, Policy::Allow) => {
            // N.b. the update is a non-fast-forward but
            // we allow it, so we pass `force: true`.
            repo.backend
                .reference(name.as_ref(), target.into(), true, "radicle: update")
                .map_err(|err| error::Update::Create {
                    name: name.to_owned(),
                    target,
                    err,
                })?;
            Ok(RefUpdate::from(name.to_ref_string(), prev, target).into())
        }
        // N.b. if the target is behind, we simply reject the update
        Some(Ancestry::Behind) => Ok(Update::Direct {
            name,
            target,
            no_ff,
        }
        .into()),
        Some(Ancestry::Diverged) | None if matches!(no_ff, Policy::Reject) => Ok(Update::Direct {
            name,
            target,
            no_ff,
        }
        .into()),
        Some(Ancestry::Diverged) | None => Err(error::Update::NonFF {
            name: name.to_owned(),
            new: target,
            cur: prev.into(),
        }),
    }
}

fn prune<'a>(
    repo: &Repository,
    name: Namespaced<'a>,
    prev: Either<Oid, Qualified<'a>>,
) -> Result<Updated<'a>, error::Update> {
    use git::raw::ObjectType;

    match find(repo, &name)? {
        Some(mut r) => {
            // N.b. peel this reference to whatever object it points to,
            // presumably a commit, and get its Oid
            let prev = r
                .peel(ObjectType::Any)
                .map_err(error::Update::Peel)?
                .id()
                .into();
            r.delete().map_err(|err| error::Update::Delete {
                name: name.to_owned(),
                err,
            })?;
            Ok(RefUpdate::Deleted {
                name: name.to_ref_string(),
                oid: prev,
            }
            .into())
        }
        None => Ok(Update::Prune { name, prev }.into()),
    }
}

fn find<'a>(
    repo: &'a Repository,
    name: &Namespaced<'_>,
) -> Result<Option<radicle::git::raw::Reference<'a>>, error::Update> {
    match repo.backend.find_reference(name.as_ref()) {
        Ok(r) => Ok(Some(r)),
        Err(e) if matches!(e.code(), radicle::git::raw::ErrorCode::NotFound) => Ok(None),
        Err(err) => Err(error::Update::Find {
            name: name.clone().into_owned(),
            err,
        }),
    }
}

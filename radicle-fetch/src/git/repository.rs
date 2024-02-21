pub mod error;

use either::Either;
use radicle::git::{self, Namespaced, Oid, Qualified};
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

impl<'a> From<RefUpdate> for Updated<'a> {
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
    match repo.backend.find_object(*oid, None) {
        Ok(object) => Ok(object
            .peel(git::raw::ObjectType::Commit)
            .map_err(|err| error::Ancestry::Peel { oid, err })?
            .id()
            .into()),
        Err(e) if git::is_not_found_err(&e) => Err(error::Ancestry::Missing { oid }),
        Err(err) => Err(error::Ancestry::Object { oid, err }),
    }
}

pub fn ancestry(repo: &Repository, old: Oid, new: Oid) -> Result<Ancestry, error::Ancestry> {
    let old = find_and_peel(repo, old)?;
    let new = find_and_peel(repo, new)?;

    if old == new {
        return Ok(Ancestry::Equal);
    }

    let (ahead, behind) = repo
        .backend
        .graph_ahead_behind(*new, *old)
        .map_err(|err| error::Ancestry::Check { old, new, err })?;

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
    use radicle::git::raw::ErrorCode::NotFound;

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
    let tip = refname_to_id(repo, name.clone())?;
    match tip {
        Some(prev) => {
            let ancestry = ancestry(repo, prev, target)?;

            match ancestry {
                Ancestry::Equal => Ok(RefUpdate::Skipped {
                    name: name.to_ref_string(),
                    oid: target,
                }
                .into()),
                Ancestry::Ahead => {
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
                Ancestry::Behind | Ancestry::Diverged if matches!(no_ff, Policy::Allow) => {
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
                Ancestry::Behind => Ok(Update::Direct {
                    name,
                    target,
                    no_ff,
                }
                .into()),
                Ancestry::Diverged if matches!(no_ff, Policy::Reject) => Ok(Update::Direct {
                    name,
                    target,
                    no_ff,
                }
                .into()),
                Ancestry::Diverged => {
                    return Err(error::Update::NonFF {
                        name: name.to_owned(),
                        new: target,
                        cur: prev,
                    })
                }
            }
        }
        None => {
            // N.b. the reference didn't exist so we pass `force:
            // false`.
            repo.backend
                .reference(name.as_ref(), target.into(), false, "radicle: create")
                .map_err(|err| error::Update::Create {
                    name: name.to_owned(),
                    target,
                    err,
                })?;
            Ok(RefUpdate::Created {
                name: name.to_ref_string(),
                oid: target,
            }
            .into())
        }
    }
}

fn prune<'a>(
    repo: &Repository,
    name: Namespaced<'a>,
    prev: Either<Oid, Qualified<'a>>,
) -> Result<Updated<'a>, error::Update> {
    use radicle::git::raw::ObjectType;

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

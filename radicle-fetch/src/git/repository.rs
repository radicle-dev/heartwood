pub mod error;

use either::{
    Either,
    Either::{Left, Right},
};
use radicle::git::{Namespaced, Oid, Qualified};
use radicle::storage::git::Repository;
use radicle::storage::ReadRepository;

use super::refs::{Applied, Policy, RefUpdate, Update};

pub fn contains(repo: &Repository, oid: Oid) -> Result<bool, error::Contains> {
    repo.backend
        .odb()
        .map(|odb| odb.exists(oid.into()))
        .map_err(error::Contains)
}

pub fn is_in_ancestry_path(repo: &Repository, old: Oid, new: Oid) -> Result<bool, error::Ancestry> {
    if !contains(repo, old)? || !contains(repo, new)? {
        return Ok(false);
    }

    if old == new {
        return Ok(true);
    }

    repo.is_ancestor_of(old, new)
        .map_err(|err| error::Ancestry::Check { old, new, err })
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
                Left(r) => applied.rejected.push(r),
                Right(u) => applied.updated.push(u),
            },
            Update::Prune { name, prev } => match prune(repo, name, prev)? {
                Left(r) => applied.rejected.push(r),
                Right(u) => applied.updated.push(u),
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
) -> Result<Either<Update<'a>, RefUpdate>, error::Update> {
    let tip = refname_to_id(repo, name.clone())?;
    match tip {
        Some(prev) => {
            let is_ff = is_in_ancestry_path(repo, prev, target)?;
            if !is_ff {
                match no_ff {
                    Policy::Abort => {
                        return Err(error::Update::NonFF {
                            name: name.to_owned(),
                            new: target,
                            cur: prev,
                        })
                    }
                    Policy::Reject => Ok(Left(Update::Direct {
                        name,
                        target,
                        no_ff,
                    })),
                    Policy::Allow => {
                        // N.b. the update is a non-fast-forward but
                        // we allow it, so we pass `force: true`.
                        repo.backend
                            .reference(name.as_ref(), target.into(), true, "radicle: update")
                            .map_err(|err| error::Update::Create {
                                name: name.to_owned(),
                                target,
                                err,
                            })?;
                        Ok(Right(RefUpdate::from(name.to_ref_string(), prev, target)))
                    }
                }
            } else {
                // N.b. the update is a fast-forward so we can safely
                // pass `force: true`.
                repo.backend
                    .reference(name.as_ref(), target.into(), true, "radicle: update")
                    .map_err(|err| error::Update::Create {
                        name: name.to_owned(),
                        target,
                        err,
                    })?;
                Ok(Right(RefUpdate::from(name.to_ref_string(), prev, target)))
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
            Ok(Right(RefUpdate::Created {
                name: name.to_ref_string(),
                oid: target,
            }))
        }
    }
}

fn prune<'a>(
    repo: &Repository,
    name: Namespaced<'a>,
    prev: Either<Oid, Qualified<'a>>,
) -> Result<Either<Update<'a>, RefUpdate>, error::Update> {
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
            Ok(Right(RefUpdate::Deleted {
                name: name.to_ref_string(),
                oid: prev,
            }))
        }
        None => Ok(Left(Update::Prune { name, prev })),
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

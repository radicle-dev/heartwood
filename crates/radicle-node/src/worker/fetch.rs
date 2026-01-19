use radicle::identity::doc::CanonicalRefsError;
use radicle::identity::CanonicalRefs;
use radicle::storage::git::TempRepository;
pub(crate) use radicle_protocol::worker::fetch::error;

use std::collections::BTreeSet;
use std::str::FromStr;

use localtime::LocalTime;

use radicle::cob::TypedId;
use radicle::crypto::PublicKey;
use radicle::identity::crefs::GetCanonicalRefs as _;
use radicle::prelude::NodeId;
use radicle::prelude::RepoId;
use radicle::storage::git::Repository;
use radicle::storage::refs::RefsAt;
use radicle::storage::{
    ReadRepository, ReadStorage as _, RefUpdate, RemoteRepository, RepositoryError,
    WriteRepository as _,
};
use radicle::{cob, git, node, Storage};
use radicle_fetch::git::refs::Applied;
use radicle_fetch::{Allowed, BlockList, FetchLimit};
pub use radicle_protocol::worker::fetch::{FetchResult, UpdatedCanonicalRefs};

use super::channels::ChannelsFlush;

pub enum Handle {
    Clone {
        handle: radicle_fetch::Handle<TempRepository, ChannelsFlush>,
    },
    Pull {
        handle: radicle_fetch::Handle<Repository, ChannelsFlush>,
        notifications: node::notifications::StoreWriter,
    },
}

impl Handle {
    pub fn new(
        rid: RepoId,
        local: PublicKey,
        storage: &Storage,
        follow: Allowed,
        blocked: BlockList,
        channels: ChannelsFlush,
        notifications: node::notifications::StoreWriter,
    ) -> Result<Self, error::Handle> {
        let exists = storage.contains(&rid)?;
        if exists {
            let repo = storage.repository(rid)?;
            let handle = radicle_fetch::Handle::new(local, repo, follow, blocked, channels)?;
            Ok(Handle::Pull {
                handle,
                notifications,
            })
        } else {
            let repo = storage.temporary_repository(rid)?;
            let handle = radicle_fetch::Handle::new(local, repo, follow, blocked, channels)?;
            Ok(Handle::Clone { handle })
        }
    }

    pub fn fetch<D: node::refs::Store>(
        self,
        rid: RepoId,
        storage: &Storage,
        cache: &mut cob::cache::StoreWriter,
        refsdb: &mut D,
        limit: FetchLimit,
        remote: PublicKey,
        refs_at: Option<Vec<RefsAt>>,
    ) -> Result<FetchResult, error::Fetch> {
        let (result, clone, notifs) = match self {
            Self::Clone { mut handle } => {
                log::debug!(target: "worker", "{} cloning from {remote}", handle.local());
                match radicle_fetch::clone(&mut handle, limit, remote) {
                    Err(err) => {
                        handle.into_inner().cleanup();
                        return Err(err.into());
                    }
                    Ok(result) => {
                        handle.into_inner().mv(storage.path_of(&rid))?;
                        (result, true, None)
                    }
                }
            }
            Self::Pull {
                mut handle,
                notifications,
            } => {
                log::debug!(target: "worker", "{} pulling from {remote}", handle.local());
                let result = radicle_fetch::pull(&mut handle, limit, remote, refs_at)?;
                (result, false, Some(notifications))
            }
        };

        for rejected in result.rejected() {
            log::debug!(target: "worker", "Rejected update for {}", rejected.refname())
        }

        match result {
            radicle_fetch::FetchResult::Failed {
                threshold,
                delegates,
                validations,
            } => {
                for fail in validations.iter() {
                    log::warn!(target: "worker", "Validation error: {fail}");
                }
                Err(error::Fetch::Validation {
                    threshold,
                    delegates: delegates.into_iter().map(|key| key.to_string()).collect(),
                })
            }
            radicle_fetch::FetchResult::Success {
                applied,
                remotes,
                validations,
            } => {
                for warn in validations {
                    log::debug!(target: "worker", "Validation error: {warn}");
                }

                // N.b. We do not go through handle for this since the cloning handle
                // points to a repository that is temporary and gets moved by [`mv`].
                let repo = storage.repository(rid)?;
                repo.set_identity_head()?;
                match repo.set_head() {
                    Ok(head) => {
                        if head.is_updated() {
                            log::trace!(target: "worker", "Set HEAD to {}", head.new);
                        }
                    }
                    Err(RepositoryError::Quorum(e)) => {
                        log::warn!(target: "worker", "Fetch could not set HEAD for {rid}: {e}")
                    }
                    Err(e) => return Err(e.into()),
                }

                let canonical = match set_canonical_refs(&repo, &applied) {
                    Ok(updates) => updates.unwrap_or_default(),
                    Err(e) => {
                        log::warn!(target: "worker", "Failed to set canonical references for {rid}: {e}");
                        UpdatedCanonicalRefs::default()
                    }
                };

                // Notifications are only posted for pulls, not clones.
                if let Some(mut store) = notifs {
                    // Only create notifications for repos that we have
                    // contributed to in some way, otherwise our inbox will
                    // be flooded by all the repos we are seeding.
                    if repo.remote(&storage.info().key).is_ok() {
                        notify(&rid, &applied, &mut store)?;
                    }
                }

                cache_cobs(&rid, &applied.updated, &repo, cache)?;
                cache_refs(&rid, &applied.updated, refsdb)?;

                Ok(FetchResult {
                    updated: applied.updated,
                    canonical,
                    namespaces: remotes.into_iter().collect(),
                    doc: repo.identity_doc()?,
                    clone,
                })
            }
        }
    }
}

// Post notifications for the given refs.
fn notify(
    rid: &RepoId,
    refs: &radicle_fetch::git::refs::Applied<'static>,
    store: &mut node::notifications::StoreWriter,
) -> Result<(), error::Fetch> {
    let now = LocalTime::now();

    for update in refs.updated.iter() {
        if let Some(r) = update.name().to_namespaced() {
            let r = r.strip_namespace();
            if r == *git::refs::storage::SIGREFS_BRANCH {
                // Don't notify about signed refs.
                continue;
            }
            if r == *git::refs::storage::IDENTITY_BRANCH {
                // Don't notify about the peers's identity branch pointer, since there will
                // be a separate notification on the identity COB itself.
                continue;
            }
            if r == *git::refs::storage::IDENTITY_ROOT {
                // Don't notify about the peers's identity root pointer. This is only used
                // for sigref verification.
                continue;
            }
            if let Some(rest) = r.strip_prefix(git::fmt::refname!("refs/heads/patches")) {
                if radicle::cob::ObjectId::from_str(rest.as_str()).is_ok() {
                    // Don't notify about patch branches, since we already get
                    // notifications about patch updates.
                    continue;
                }
            }
        }
        if let RefUpdate::Skipped { .. } = update {
            // Don't notify about skipped refs.
        } else if let Err(e) = store.insert(rid, update, now) {
            log::debug!(
                target: "worker",
                "Failed to update notification store for {rid}: {e}"
            );
        }
    }
    Ok(())
}

/// Cache certain ref updates in our database.
fn cache_refs<D>(repo: &RepoId, refs: &[RefUpdate], db: &mut D) -> Result<(), node::refs::Error>
where
    D: node::refs::Store,
{
    let time = LocalTime::now();

    for r in refs {
        let name = r.name();
        let (namespace, qualified) = match radicle::git::parse_ref_namespaced(name) {
            Err(e) => {
                log::debug!(target: "worker", "Git reference is invalid: {name:?}: {e}");
                log::debug!(target: "worker", "Skipping refs caching for fetch of {repo}");
                break;
            }
            Ok((n, q)) => (n, q),
        };
        if qualified != *git::refs::storage::SIGREFS_BRANCH {
            // Only cache `rad/sigrefs`.
            continue;
        }
        log::trace!(target: "node", "Updating cache for {name} in {repo}");

        let result = match r {
            RefUpdate::Updated { new, .. } => db.set(repo, &namespace, &qualified, *new, time),
            RefUpdate::Created { oid, .. } => db.set(repo, &namespace, &qualified, *oid, time),
            RefUpdate::Deleted { .. } => db.delete(repo, &namespace, &qualified),
            RefUpdate::Skipped { .. } => continue,
        };

        if let Err(e) = result {
            log::debug!(target: "worker", "Failed to update git refs cache for {name:?}: {e}");
            log::debug!(target: "worker", "Skipping refs caching for fetch of {repo}");
            break;
        }
    }
    Ok(())
}

/// Write new `RefUpdate`s that are related a `Patch` or an `Issue`
/// COB to the COB cache.
fn cache_cobs<S, C>(
    rid: &RepoId,
    refs: &[RefUpdate],
    storage: &S,
    cache: &mut C,
) -> Result<(), error::Cache>
where
    S: ReadRepository + cob::Store<Namespace = NodeId>,
    C: cob::cache::Update<cob::issue::Issue> + cob::cache::Update<cob::patch::Patch>,
    C: cob::cache::Remove<cob::issue::Issue> + cob::cache::Remove<cob::patch::Patch>,
{
    let mut issues = cob::store::Store::<cob::issue::Issue, _>::open(storage)?;
    let mut patches = cob::store::Store::<cob::patch::Patch, _>::open(storage)?;

    for update in refs {
        match update {
            RefUpdate::Updated { name, .. }
            | RefUpdate::Created { name, .. }
            | RefUpdate::Deleted { name, .. } => match name.to_namespaced() {
                Some(name) => {
                    let Some(identifier) = cob::TypedId::from_namespaced(&name)? else {
                        continue;
                    };
                    if identifier.is_issue() {
                        update_or_remove(&mut issues, cache, rid, identifier)?;
                    } else if identifier.is_patch() {
                        update_or_remove(&mut patches, cache, rid, identifier)?;
                    } else {
                        // Unknown COB, don't cache.
                        continue;
                    }
                }
                None => continue,
            },
            RefUpdate::Skipped { .. } => { /* Do nothing */ }
        }
    }

    Ok(())
}

/// Update or remove a cache entry.
fn update_or_remove<R, C, T>(
    store: &mut cob::store::Store<T, R>,
    cache: &mut C,
    rid: &RepoId,
    tid: TypedId,
) -> Result<(), error::Cache>
where
    R: cob::Store + ReadRepository,
    T: cob::Evaluate<R> + cob::store::Cob + cob::store::CobWithType,
    C: cob::cache::Update<T> + cob::cache::Remove<T>,
{
    match store.get(&tid.id) {
        Ok(Some(obj)) => {
            // Object loaded correctly, update cache.
            return cache.update(rid, &tid.id, &obj).map(|_| ()).map_err(|e| {
                error::Cache::Update {
                    id: tid.id,
                    type_name: tid.type_name,
                    err: e.into(),
                }
            });
        }
        Ok(None) => {
            // Object was not found. Fall-through.
        }
        Err(e) => {
            // Object was found, but failed to load. Fall-through.
            log::debug!(target: "fetch", "Failed to load COB {tid} from storage: {e}");
        }
    }
    // The object has either been removed entirely from the repository,
    // or it failed to load. So we also remove it from the cache.
    cob::cache::Remove::<T>::remove(cache, &tid.id)
        .map(|_| ())
        .map_err(|e| error::Cache::Remove {
            id: tid.id,
            type_name: tid.type_name,
            err: Box::new(e),
        })?;

    Ok(())
}

fn set_canonical_refs(
    repo: &Repository,
    applied: &Applied,
) -> Result<Option<UpdatedCanonicalRefs>, error::Canonical> {
    let identity = repo.identity()?;
    // TODO(finto): it's unfortunate that we may end up computing the default
    // branch again after `set_head` is called after the fetch. This is due to
    // the storage capabilities being leaked to this part of the code base.
    let rules = identity
        .canonical_refs_or_default(|| {
            let rule = identity.doc().default_branch_rule()?;
            Ok::<_, CanonicalRefsError>(CanonicalRefs::from_iter([rule]))
        })?
        .rules()
        .clone();

    let mut updated_refs = UpdatedCanonicalRefs::default();
    let refnames = applied
        .updated
        .iter()
        .filter_map(|update| match update {
            RefUpdate::Updated { name, .. } | RefUpdate::Created { name, .. } => {
                let name = name.clone().into_qualified()?;
                let name = name.to_namespaced()?;
                Some(name.strip_namespace())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for name in refnames {
        let canonical = match rules.canonical(name.clone(), repo) {
            Some(canonical) => canonical,
            None => continue,
        };

        let canonical = match canonical.find_objects() {
            Err(err) => {
                log::warn!(target: "worker", "Failed to find objects for canonical computation of `{name}`: {err}");
                continue;
            }
            Ok(canonical) => canonical,
        };

        match canonical.quorum() {
            Err(err) => {
                log::warn!(
                    target: "worker",
                    "Failed to calculate canonical reference `{name}`: {err}",
                );
                continue;
            }
            Ok(git::canonical::Quorum {
                refname, object, ..
            }) => {
                let oid = object.id();
                if let Err(e) = repo.backend.reference(
                    refname.clone().as_str(),
                    oid.into(),
                    true,
                    "set-canonical-reference from fetch (radicle)",
                ) {
                    log::warn!(
                        target: "worker",
                        "Failed to set canonical reference {refname}->{oid}: {e}"
                    );
                } else {
                    updated_refs.updated(refname, oid);
                }
            }
        }
    }
    Ok(Some(updated_refs))
}

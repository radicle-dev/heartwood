pub mod error;

use std::collections::HashSet;
use std::str::FromStr;

use localtime::LocalTime;

use radicle::crypto::PublicKey;
use radicle::identity::DocAt;
use radicle::prelude::RepoId;
use radicle::storage::refs::RefsAt;
use radicle::storage::{
    ReadRepository, ReadStorage as _, RefUpdate, RemoteRepository, WriteRepository as _,
};
use radicle::{cob, git, node, Storage};
use radicle_fetch::{Allowed, BlockList, FetchLimit};

use super::channels::ChannelsFlush;

#[derive(Debug, Clone)]
pub struct FetchResult {
    /// The set of updated references.
    pub updated: Vec<RefUpdate>,
    /// The set of remote namespaces that were updated.
    pub namespaces: HashSet<PublicKey>,
    /// The fetch was a full clone.
    pub clone: bool,
    /// Identity doc of fetched repo.
    pub doc: DocAt,
}

impl FetchResult {
    pub fn new(doc: DocAt) -> Self {
        Self {
            updated: vec![],
            namespaces: HashSet::new(),
            clone: false,
            doc,
        }
    }
}

pub enum Handle {
    Clone {
        handle: radicle_fetch::Handle<ChannelsFlush>,
        tmp: tempfile::TempDir,
    },
    Pull {
        handle: radicle_fetch::Handle<ChannelsFlush>,
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
            let (repo, tmp) = storage.lock_repository(rid)?;
            let handle = radicle_fetch::Handle::new(local, repo, follow, blocked, channels)?;
            Ok(Handle::Clone { handle, tmp })
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
            Self::Clone { mut handle, tmp } => {
                log::debug!(target: "worker", "{} cloning from {remote}", handle.local());
                let result = radicle_fetch::clone(&mut handle, limit, remote)?;
                mv(tmp, storage, &rid)?;
                (result, true, None)
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
            log::warn!(target: "worker", "Rejected update for {}", rejected.refname())
        }

        match result {
            radicle_fetch::FetchResult::Failed {
                threshold,
                delegates,
                validations,
            } => {
                for fail in validations.iter() {
                    log::error!(target: "worker", "Validation error: {}", fail);
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
                    log::warn!(target: "worker", "Validation error: {}", warn);
                }

                // N.b. We do not go through handle for this since the cloning handle
                // points to a repository that is temporary and gets moved by [`mv`].
                let repo = storage.repository(rid)?;
                repo.set_identity_head()?;
                repo.set_head()?;

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
                    namespaces: remotes.into_iter().collect(),
                    doc: repo.identity_doc()?,
                    clone,
                })
            }
        }
    }
}

/// In the case of cloning, we have performed the fetch into a
/// temporary directory -- ensuring that no concurrent operations
/// see an empty repository.
///
/// At the end of the clone, we perform a rename of the temporary
/// directory to the storage repository.
///
/// # Errors
///   - Will fail if `storage` contains `rid` already.
fn mv(tmp: tempfile::TempDir, storage: &Storage, rid: &RepoId) -> Result<(), error::Fetch> {
    use std::io::{Error, ErrorKind};

    let from = tmp.path();
    let to = storage.path_of(rid);

    if !to.exists() {
        std::fs::rename(from, to)?;
    } else {
        log::warn!(target: "worker", "Refusing to move cloned repository {rid} already exists");
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("repository already exists {:?}", to),
        )
        .into());
    }

    Ok(())
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
            if let Some(rest) = r.strip_prefix(git::refname!("refs/heads/patches")) {
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
            log::error!(
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
                log::error!(target: "worker", "Git reference is invalid: {name:?}: {e}");
                log::warn!(target: "worker", "Skipping refs caching for fetch of {repo}");
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
            log::error!(target: "worker", "Error updating git refs cache for {name:?}: {e}");
            log::warn!(target: "worker", "Skipping refs caching for fetch of {repo}");
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
    S: ReadRepository + cob::Store,
    C: cob::cache::Update<cob::issue::Issue> + cob::cache::Update<cob::patch::Patch>,
    C: cob::cache::Remove<cob::issue::Issue> + cob::cache::Remove<cob::patch::Patch>,
{
    let issues = cob::issue::Issues::open(storage)?;
    let patches = cob::patch::Patches::open(storage)?;
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
                        if let Some(issue) = issues.get(&identifier.id)? {
                            cache
                                .update(rid, &identifier.id, &issue)
                                .map(|_| ())
                                .map_err(|e| error::Cache::Update {
                                    id: identifier.id,
                                    type_name: identifier.type_name,
                                    err: e.into(),
                                })?;
                        } else {
                            // N.b. the issue has been removed entirely from the
                            // repository so we also remove it from the cache
                            cob::cache::Remove::<cob::issue::Issue>::remove(cache, &identifier.id)
                                .map(|_| ())
                                .map_err(|e| error::Cache::Remove {
                                    id: identifier.id,
                                    type_name: identifier.type_name,
                                    err: Box::new(e),
                                })?;
                        }
                    } else if identifier.is_patch() {
                        if let Some(patch) = patches.get(&identifier.id)? {
                            cache
                                .update(rid, &identifier.id, &patch)
                                .map(|_| ())
                                .map_err(|e| error::Cache::Update {
                                    id: identifier.id,
                                    type_name: identifier.type_name,
                                    err: e.into(),
                                })?;
                        } else {
                            // N.b. the patch has been removed entirely from the
                            // repository so we also remove it from the cache
                            cob::cache::Remove::<cob::patch::Patch>::remove(cache, &identifier.id)
                                .map(|_| ())
                                .map_err(|e| error::Cache::Remove {
                                    id: identifier.id,
                                    type_name: identifier.type_name,
                                    err: Box::new(e),
                                })?;
                        }
                    }
                }
                None => continue,
            },
            RefUpdate::Skipped { .. } => { /* Do nothing */ }
        }
    }

    Ok(())
}

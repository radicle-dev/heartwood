mod refspecs;
pub use refspecs::{AsRefspecs, Refspec, SpecialRefs};

pub mod error;

use std::collections::{BTreeMap, HashSet};
use std::ops::Deref;

use radicle::crypto::{PublicKey, Unverified, Verified};
use radicle::git::url;
use radicle::prelude::{Doc, Id};
use radicle::storage::git::Repository;
use radicle::storage::refs::{SignedRefs, IDENTITY_BRANCH};
use radicle::storage::{Namespaces, RefUpdate, Remote, RemoteId};
use radicle::storage::{ReadRepository, ReadStorage, WriteRepository, WriteStorage};
use radicle::{git, storage, Storage};

/// The initial phase of staging a fetch from a remote.
///
/// The [`StagingPhaseInitial::refpsecs`] generated are to fetch the
/// `rad/id` and/or `rad/sigrefs` references from the remote end.
///
/// It is then expected to convert this into [`StagingPhaseFinal`]
/// using [`StagingRad::into_final`] to continue the rest of the
/// references.
pub struct StagingPhaseInitial<'a> {
    /// The inner [`Repository`] for staging fetches into.
    pub(super) repo: StagedRepository,
    /// The original [`Storage`] we are finalising changes into.
    production: &'a Storage,
    /// The `Namespaces` passed by the fetching caller.
    namespaces: Namespaces,
    _tmp: tempfile::TempDir,
}

/// Indicates whether the innner [`Repository`] is being cloned into
/// or fetched into.
pub enum StagedRepository {
    Cloning(Repository),
    Fetching(Repository),
}

impl Deref for StagedRepository {
    type Target = Repository;

    fn deref(&self) -> &Self::Target {
        match self {
            StagedRepository::Cloning(repo) => repo,
            StagedRepository::Fetching(repo) => repo,
        }
    }
}

/// The second, and final, phase of staging a fetch from a remote.
///
/// The [`StagingPhaseFinal::refpsecs`] generated are to fetch any follow-up
/// references after the fetch on [`StagingPhaseInitial`]. This may be all the
/// delegate's references in the case of cloning the new repository,
/// or it could be fetching the latest updates in the case of fetching
/// an existing repository.
///
/// It is then expected to finalise the process by transferring the
/// fetched references into the production storage, via
/// [`StagingPhaseFinal::transfer`].
pub struct StagingPhaseFinal<'a> {
    /// The inner [`Repository`] for staging fetches into.
    pub(super) repo: StagedRepository,
    /// The original [`Storage`] we are finalising changes into.
    production: &'a Storage,
    /// The delegates and tracked remotes that the fetch is being
    /// performed for. These are passed through from the
    /// [`StagingPhaseInitial::namespaces`], if the variant is `Trusted`.
    trusted: HashSet<RemoteId>,
    _tmp: tempfile::TempDir,
}

enum VerifiedRemote {
    Failed {
        reason: String,
    },
    Success {
        // Nb. unused but we want to ensure that we verify the identity
        _doc: Doc<Verified>,
        remote: Remote<Verified>,
    },
}

impl<'a> StagingPhaseInitial<'a> {
    /// Construct a [`StagingPhaseInitial`] which sets up its
    /// [`StagedRepository`] in a new, temporary directory.
    pub fn new(
        production: &'a Storage,
        rid: Id,
        namespaces: Namespaces,
    ) -> Result<Self, error::Init> {
        let tmp = tempfile::TempDir::new()?;
        log::debug!(target: "worker", "Staging fetch in {:?}", tmp.path());
        let staging = Storage::open(tmp.path())?;
        let repo = Self::repository(&staging, production, rid)?;
        Ok(Self {
            repo,
            production,
            namespaces,
            _tmp: tmp,
        })
    }

    /// Return the fetch refspecs for fetching the necessary `rad`
    /// references.
    pub fn refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        let id = git::PatternString::from(IDENTITY_BRANCH.clone().into_refstring());
        match self.repo {
            StagedRepository::Cloning(_) => Refspec {
                src: id.clone(),
                dst: id,
                force: false,
            }
            .into_refspecs(),
            StagedRepository::Fetching(_) => SpecialRefs(self.namespaces.clone()).into_refspecs(),
        }
    }

    /// Convert the [`StagingPhaseInitial`] into [`StagingPhaseFinal`] to continue
    /// the fetch process.
    pub fn into_final(self) -> Result<StagingPhaseFinal<'a>, error::Transition> {
        let trusted = match &self.repo {
            StagedRepository::Cloning(repo) => {
                log::debug!(target: "worker", "Loading remotes for clone");
                let oid = ReadRepository::identity_head(repo)?;
                log::trace!(target: "worker", "Loading 'rad/id' @ {oid}");
                let (doc, _) = Doc::<Unverified>::load_at(oid, repo)?;
                let doc = doc.verified()?;
                let mut trusted = match self.namespaces.clone() {
                    Namespaces::All => HashSet::new(),
                    Namespaces::Trusted(trusted) => trusted,
                };
                let delegates = doc.delegates.map(PublicKey::from);
                trusted.extend(delegates);
                trusted
            }
            StagedRepository::Fetching(repo) => {
                log::debug!(target: "worker", "Loading remotes for fetching");
                match self.namespaces.clone() {
                    Namespaces::All => {
                        let mut trusted = repo.remote_ids()?.collect::<Result<HashSet<_>, _>>()?;
                        trusted.extend(repo.delegates()?.map(PublicKey::from));
                        trusted
                    }

                    Namespaces::Trusted(trusted) => trusted,
                }
            }
        };

        Ok(StagingPhaseFinal {
            repo: self.repo,
            production: self.production,
            trusted,
            _tmp: self._tmp,
        })
    }

    fn repository(
        staging: &Storage,
        production: &Storage,
        rid: Id,
    ) -> Result<StagedRepository, error::Setup> {
        match production.contains(&rid) {
            Ok(true) => {
                let url = url::File::new(production.path_of(&rid)).to_string();
                log::debug!(target: "worker", "Setting up fetch for existing repository: {}", url);
                let to = storage::git::paths::repository(&staging, &rid);
                let copy = git::raw::build::RepoBuilder::new()
                    .bare(true)
                    .clone_local(git::raw::build::CloneLocal::Local)
                    .clone(&url, &to)?;

                Ok(StagedRepository::Fetching(Repository {
                    id: rid,
                    backend: copy,
                }))
            }
            Ok(false) => {
                log::debug!(target: "worker", "Setting up clone for new repository {}", rid);
                let repo = staging.create(rid)?;
                Ok(StagedRepository::Cloning(repo))
            }
            Err(e) => Err(e.into()),
        }
    }
}

impl<'a> StagingPhaseFinal<'a> {
    /// Return the fetch refspecs for fetching the necessary
    /// references.
    pub fn refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        match self.repo {
            StagedRepository::Cloning(_) => Namespaces::Trusted(self.trusted.clone()).as_refspecs(),
            StagedRepository::Fetching(_) => {
                self.remotes().fold(Vec::new(), |mut specs, remote| {
                    specs.extend(remote.as_refspecs());
                    specs
                })
            }
        }
    }

    /// Finalise the fetching process via the following steps.
    ///
    /// Verify all `rad/id` and `rad/sigrefs` from fetched
    /// remotes. Any remotes that fail will be ignored and not fetched
    /// into the production repository.
    ///
    /// For each remote that verifies, fetch from the staging storage
    /// into the production storage using the refspec:
    ///
    /// ```text
    /// refs/namespaces/<remote>/*:refs/namespaces/<remote>/*
    /// ```
    ///
    /// All references that were updated are returned as a
    /// [`RefUpdate`].
    pub fn transfer(self) -> Result<Vec<RefUpdate>, error::Transfer> {
        let verifications = self.verify();
        let production = match &self.repo {
            StagedRepository::Cloning(repo) => self.production.create(repo.id)?,
            StagedRepository::Fetching(repo) => self.production.repository(repo.id)?,
        };
        let url = url::File::new(self.repo.path().to_path_buf()).to_string();
        let mut remote = production.backend.remote_anonymous(&url)?;
        let mut updates = Vec::new();

        let callbacks = ref_updates(&mut updates);
        {
            let specs = verifications
                .into_iter()
                .filter_map(|(remote, verified)| match verified {
                    VerifiedRemote::Failed { reason } => {
                        log::warn!(
                            target: "worker",
                            "{remote} failed to verify, will not fetch any further refs: {reason}",
                        );
                        None
                    }
                    VerifiedRemote::Success { remote, .. } => {
                        let ns = remote.id.to_namespace().with_pattern(git::refspec::STAR);
                        Some(
                            Refspec {
                                src: ns.clone(),
                                dst: ns,
                                force: false,
                            }
                            .to_string(),
                        )
                    }
                })
                .collect::<Vec<_>>();
            log::debug!(target: "worker", "Transferring staging to production {url}");

            let mut opts = git::raw::FetchOptions::default();
            opts.remote_callbacks(callbacks);
            // Nb. To prevent refs owned by the local node from being deleted from the stored
            // copy if they are not on the remote side, we turn pruning off.
            // However, globally turning off pruning isn't a ideal either, so a better solution
            // should be devised.
            opts.prune(git::raw::FetchPrune::Off);

            remote.fetch(&specs, Some(&mut opts), None)?;
        }
        let head = production.set_head()?;
        log::debug!(target: "worker", "Head for {} set to {head}", production.id);

        let head = production.set_identity_head()?;
        log::debug!(target: "worker", "'refs/rad/id' for {} set to {head}", production.id);

        Ok(updates)
    }

    fn remotes(&self) -> impl Iterator<Item = Remote> + '_ {
        self.trusted
            .iter()
            .filter_map(|remote| match SignedRefs::load(remote, self.repo.deref()) {
                Ok(refs) => Some(Remote::new(*remote, refs)),
                Err(err) => {
                    log::warn!(target: "worker", "{remote} failed rad/sigrefs verification: {err}");
                    None
                }
            })
    }

    fn verify(&self) -> BTreeMap<RemoteId, VerifiedRemote> {
        self.trusted
            .iter()
            .map(|remote| {
                let verification = match (
                    self.repo.identity_doc_of(remote),
                    SignedRefs::load(remote, self.repo.deref()),
                ) {
                    (Ok(doc), Ok(refs)) => VerifiedRemote::Success {
                        _doc: doc,
                        remote: Remote::new(*remote, refs),
                    },
                    (Err(e), _) => VerifiedRemote::Failed {
                        reason: e.to_string(),
                    },
                    (_, Err(e)) => VerifiedRemote::Failed {
                        reason: e.to_string(),
                    },
                };
                (*remote, verification)
            })
            .collect()
    }
}

fn ref_updates(updates: &mut Vec<RefUpdate>) -> git::raw::RemoteCallbacks<'_> {
    let mut callbacks = git::raw::RemoteCallbacks::new();
    callbacks.update_tips(|name, old, new| {
        if let Ok(name) = git::RefString::try_from(name) {
            if name.to_namespaced().is_some() {
                updates.push(RefUpdate::from(name, old, new));
                // Returning `true` ensures the process is not aborted.
                return true;
            }
        }
        log::warn!(target: "worker", "Invalid ref `{}` detected; aborting fetch", name);

        false
    });
    callbacks
}

mod refspecs;
pub use refspecs::{AsRefspecs, Refspec, SpecialRefs};

pub mod error;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Deref;

use radicle::crypto::{PublicKey, Unverified, Verified};
use radicle::git::{url, Namespaced};
use radicle::prelude::{Doc, Id, NodeId};
use radicle::storage::git::Repository;
use radicle::storage::refs::IDENTITY_BRANCH;
use radicle::storage::{Namespaces, RefUpdate, Remote, RemoteId};
use radicle::storage::{ReadRepository, ReadStorage, WriteRepository, WriteStorage};
use radicle::{git, Storage};

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
    pub(super) namespaces: Namespaces,
    _tmp: tempfile::TempDir,
}

/// Indicates whether the innner [`Repository`] is being cloned into
/// or fetched into.
pub enum StagedRepository {
    Cloning(Repository),
    Fetching(Repository),
}

impl StagedRepository {
    pub fn is_cloning(&self) -> bool {
        matches!(self, Self::Cloning(_))
    }
}

impl Deref for StagedRepository {
    type Target = Repository;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Cloning(repo) => repo,
            Self::Fetching(repo) => repo,
        }
    }
}

pub enum FinalStagedRepository {
    Cloning {
        repo: Repository,
        trusted: HashSet<NodeId>,
    },
    Fetching {
        repo: Repository,
        refs: BTreeSet<Namespaced<'static>>,
    },
}

impl FinalStagedRepository {
    pub fn is_cloning(&self) -> bool {
        matches!(self, Self::Cloning { .. })
    }
}

impl Deref for FinalStagedRepository {
    type Target = Repository;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Cloning { repo, .. } => repo,
            Self::Fetching { repo, .. } => repo,
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
    pub(super) repo: FinalStagedRepository,
    /// The original [`Storage`] we are finalising changes into.
    production: &'a Storage,
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
        /// Unsigned refs.
        unsigned: Vec<git::RefString>,
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

    pub fn ls_remote_refs(&self) -> Vec<git::PatternString> {
        match &self.namespaces {
            Namespaces::All => {
                vec![git::refspec::pattern!("refs/namespaces/*")]
            }
            Namespaces::Trusted(trusted) => trusted
                .iter()
                .map(|ns| {
                    git::refname!("refs/namespaces")
                        .join(git::Component::from(ns))
                        .with_pattern(git::refspec::STAR)
                })
                .collect::<Vec<_>>(),
        }
    }

    /// Convert the [`StagingPhaseInitial`] into [`StagingPhaseFinal`] to continue
    /// the fetch process.
    pub fn into_final(
        self,
        refs: BTreeSet<Namespaced<'static>>,
    ) -> Result<StagingPhaseFinal<'a>, error::Transition> {
        let repo = match self.repo {
            StagedRepository::Cloning(repo) => {
                log::debug!(target: "worker", "Loading remotes for clone of {}", repo.id);
                let oid = ReadRepository::identity_head(&repo)?;
                log::trace!(target: "worker", "Loading 'rad/id' @ {oid}");
                let (doc, _) = Doc::<Unverified>::load_at(oid, &repo)?;
                let doc = doc.verified()?;
                let mut trusted = match self.namespaces.clone() {
                    Namespaces::All => HashSet::new(),
                    Namespaces::Trusted(trusted) => trusted,
                };
                let delegates = doc.delegates.map(PublicKey::from);
                trusted.extend(delegates);
                FinalStagedRepository::Cloning { repo, trusted }
            }
            StagedRepository::Fetching(repo) => FinalStagedRepository::Fetching { repo, refs },
        };

        Ok(StagingPhaseFinal {
            repo,
            production: self.production,
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

                let to = staging.path_of(&rid);
                let copy = git::raw::build::RepoBuilder::new()
                    .bare(true)
                    .clone_local(git::raw::build::CloneLocal::Local)
                    .clone(&url, &to)?;

                {
                    // The clone doesn't actually clone all refs, it only creates a ref for the
                    // default branch; so we explicitly fetch the rest of the refs, so they
                    // don't need to be re-fetched from the remote.
                    let mut remote = copy.remote_anonymous(&url)?;
                    let refspecs: Vec<_> = Namespaces::All
                        .into_refspecs()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect();
                    remote.fetch(&refspecs, None, None)?;
                }
                log::debug!(target: "worker", "Local clone successful for {rid}");

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
        match &self.repo {
            FinalStagedRepository::Cloning { trusted, .. } => {
                Namespaces::Trusted(trusted.clone()).as_refspecs()
            }
            FinalStagedRepository::Fetching { refs, .. } => refs.as_refspecs(),
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
    pub fn transfer(self) -> Result<(Vec<RefUpdate>, HashSet<NodeId>), error::Transfer> {
        let verifications = self.verify()?;
        let production = match &self.repo {
            FinalStagedRepository::Cloning { repo, .. } => self.production.create(repo.id)?,
            FinalStagedRepository::Fetching { repo, .. } => self.production.repository(repo.id)?,
        };
        let url = url::File::new(self.repo.path().to_path_buf()).to_string();
        let mut remote = production.backend.remote_anonymous(&url)?;
        let mut updates = Vec::new();
        let mut delete = HashSet::new();

        let callbacks = ref_updates(&mut updates);
        let remotes = {
            let specs = verifications
                .into_iter()
                .flat_map(|(remote, verified)| match verified {
                    VerifiedRemote::Failed { reason } => {
                        // TODO: We should include the skipped remotes in the fetch result,
                        // with the reason why they're skipped.
                        log::warn!(
                            target: "worker",
                            "{remote} failed to verify, will not fetch any further refs: {reason}",
                        );
                        vec![]
                    }
                    VerifiedRemote::Success {
                        remote, unsigned, ..
                    } => {
                        let ns = remote.id.to_namespace();
                        let mut refspecs = vec![];

                        // Unsigned refs should be deleted.
                        delete.insert((remote.id, unsigned));

                        //  First add the standard git refs.
                        let heads = ns.join(git::refname!("refs/heads"));
                        let cobs = ns.join(git::refname!("refs/cobs"));
                        let tags = ns.join(git::refname!("refs/tags"));
                        let notes = ns.join(git::refname!("refs/notes"));

                        for refname in [heads, cobs, tags, notes] {
                            let pattern = refname.with_pattern(git::refspec::STAR);
                            refspecs.push((
                                remote.id,
                                Refspec {
                                    src: pattern.clone(),
                                    dst: pattern,
                                    force: true,
                                }
                                .to_string(),
                            ));
                        }

                        // Then add the special refs.
                        let id = ns.join(&*radicle::git::refs::storage::IDENTITY_BRANCH);
                        let sigrefs = ns.join(&*radicle::git::refs::storage::SIGREFS_BRANCH);

                        refspecs.push((
                            remote.id,
                            Refspec {
                                src: id.clone(),
                                dst: id,
                                // Nb. The identity branch is allowed to be force-updated.
                                force: true,
                            }
                            .to_string(),
                        ));
                        refspecs.push((
                            remote.id,
                            Refspec {
                                src: sigrefs.clone(),
                                dst: sigrefs,
                                // Nb. Sigrefs are never force-updated.
                                force: false,
                            }
                            .to_string(),
                        ));
                        refspecs
                    }
                })
                .collect::<Vec<_>>();

            let (fetching, specs): (HashSet<_>, Vec<_>) = specs.into_iter().unzip();

            if !self
                .repo
                .delegates()?
                .iter()
                .all(|d| fetching.contains(d.as_key()))
            {
                return Err(error::Transfer::NoDelegates);
            }
            log::debug!(target: "worker", "Transferring staging to production {url}");

            let mut opts = git::raw::FetchOptions::default();
            opts.remote_callbacks(callbacks);
            // Nb. To prevent refs owned by the local node from being deleted from the stored
            // copy if they are not on the remote side, we turn pruning off.
            // However, globally turning off pruning isn't a ideal either, so a better solution
            // should be devised.
            opts.prune(git::raw::FetchPrune::Off);

            // Fetch into production copy.
            remote.fetch(&specs, Some(&mut opts), None)?;

            // Delete unsigned refs.
            for (namespace, unsigned) in delete {
                for refstr in unsigned {
                    let q = git::Qualified::from_refstr(&refstr)
                        .expect("StagingPhaseFinal::transfer: unsigned references are qualified");

                    if let Ok(mut r) = production.reference(&namespace, &q) {
                        log::debug!(target: "worker", "Deleting unsigned ref {namespace}/{q}..");

                        r.delete()?;
                    }
                }
            }
            fetching
        };
        let head = production.set_head()?;
        log::debug!(target: "worker", "Head for {} set to {head}", production.id);

        let head = production.set_identity_head()?;
        log::debug!(target: "worker", "'refs/rad/id' for {} set to {head}", production.id);

        #[cfg(test)]
        // N.b. This is to prevent us from shooting ourselves in the
        // foot with storage inconsistencies.
        radicle::debug_assert_matches!(
            production.validate(),
            Ok(()),
            "repository {} is not valid",
            production.id,
        );

        Ok((updates, remotes))
    }

    fn remotes(&self) -> Result<Box<dyn Iterator<Item = Remote> + '_>, git::raw::Error> {
        match &self.repo {
            FinalStagedRepository::Cloning { trusted, .. } => Ok(Box::new(
                trusted
                    .iter()
                    .filter_map(|remote| self.repo.remote(remote).ok()),
            )),
            FinalStagedRepository::Fetching { repo, .. } => Ok(Box::new(
                repo.remotes()?.filter_map(|r| r.ok().map(|(_, r)| r)),
            )),
        }
    }

    fn verify(&self) -> Result<BTreeMap<RemoteId, VerifiedRemote>, git::raw::Error> {
        Ok(self
            .remotes()?
            .map(|remote| {
                let remote_id = remote.id;
                let verification = match self.repo.identity_doc_of(&remote_id) {
                    Ok(doc) => match self.repo.validate_remote(&remote) {
                        Ok(unsigned) => VerifiedRemote::Success {
                            _doc: doc,
                            remote,
                            unsigned,
                        },
                        Err(e) => VerifiedRemote::Failed {
                            reason: e.to_string(),
                        },
                    },
                    Err(e) => VerifiedRemote::Failed {
                        reason: e.to_string(),
                    },
                };
                (remote_id, verification)
            })
            .collect())
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

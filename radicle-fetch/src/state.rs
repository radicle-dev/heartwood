use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use gix_protocol::handshake;
use radicle::crypto::PublicKey;
use radicle::git::{Oid, Qualified};
use radicle::identity::{Did, Doc, DocError};

use radicle::prelude::Verified;
use radicle::storage;
use radicle::storage::refs::{RefsAt, SignedRefs};
use radicle::storage::{
    git::Validation, Remote, RemoteId, RemoteRepository, Remotes, ValidateRepository, Validations,
};

use crate::git;
use crate::git::refs::{Applied, Update};
use crate::git::repository;
use crate::sigrefs::SignedRefsAt;
use crate::stage;
use crate::stage::ProtocolStage;
use crate::{refs, sigrefs, transport, Handle};

/// The data size limit, 5Mb, while fetching the special refs,
/// i.e. `rad/id` and `rad/sigrefs`.
pub const DEFAULT_FETCH_SPECIAL_REFS_LIMIT: u64 = 1024 * 1024 * 5;
/// The data size limit, 5Gb, while fetching the data refs,
/// i.e. `refs/heads`, `refs/tags`, `refs/cobs`, etc.
pub const DEFAULT_FETCH_DATA_REFS_LIMIT: u64 = 1024 * 1024 * 1024 * 5;

pub mod error {
    use std::io;

    use radicle::git::Oid;
    use radicle::prelude::PublicKey;
    use thiserror::Error;

    use crate::{git, git::repository, handle, sigrefs, stage};

    #[derive(Debug, Error)]
    pub enum Step {
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        Layout(#[from] stage::error::Layout),
        #[error(transparent)]
        Prepare(#[from] stage::error::Prepare),
        #[error(transparent)]
        WantsHaves(#[from] stage::error::WantsHaves),
    }

    #[derive(Debug, Error)]
    pub enum Protocol {
        #[error(transparent)]
        Ancestry(#[from] repository::error::Ancestry),
        #[error(transparent)]
        Canonical(#[from] Canonical),
        #[error("delegate '{remote}' has diverged 'rad/sigrefs': {current} -> {received}")]
        Diverged {
            remote: PublicKey,
            current: Oid,
            received: Oid,
        },
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("canonical 'refs/rad/id' is missing")]
        MissingRadId,
        #[error(transparent)]
        RefdbUpdate(#[from] repository::error::Update),
        #[error(transparent)]
        Resolve(#[from] repository::error::Resolve),
        #[error(transparent)]
        Refs(#[from] radicle::storage::refs::Error),
        #[error(transparent)]
        RemoteRefs(#[from] sigrefs::error::RemoteRefs),
        #[error("failed to get remote namespaces: {0}")]
        RemoteIds(#[source] radicle::git::raw::Error),
        #[error(transparent)]
        Step(#[from] Step),
        #[error(transparent)]
        Tracking(#[from] handle::error::Tracking),
        #[error(transparent)]
        Validation(#[from] radicle::storage::Error),
    }

    #[derive(Debug, Error)]
    pub enum Canonical {
        #[error(transparent)]
        Resolve(#[from] git::repository::error::Resolve),
        #[error(transparent)]
        Verified(#[from] radicle::identity::DocError),
    }
}

type IdentityTips = BTreeMap<PublicKey, Oid>;
type SigrefTips = BTreeMap<PublicKey, Oid>;

#[derive(Clone, Copy, Debug)]
pub struct FetchLimit {
    pub special: u64,
    pub refs: u64,
}

impl Default for FetchLimit {
    fn default() -> Self {
        Self {
            special: DEFAULT_FETCH_SPECIAL_REFS_LIMIT,
            refs: DEFAULT_FETCH_DATA_REFS_LIMIT,
        }
    }
}

#[derive(Debug)]
pub enum FetchResult {
    Success {
        /// The set of applied changes to the reference store.
        applied: Applied<'static>,
        /// The set of namespaces that were fetched.
        remotes: BTreeSet<PublicKey>,
        /// Any validation errors that were found while fetching.
        validations: sigrefs::Validations,
    },
    Failed {
        /// The threshold that needed to be met.
        threshold: usize,
        /// The offending delegates.
        delegates: BTreeSet<PublicKey>,
        /// Validation errors that were found while fetching.
        validations: sigrefs::Validations,
    },
}

impl FetchResult {
    pub fn rejected(&self) -> impl Iterator<Item = &Update<'static>> {
        match self {
            Self::Success { applied, .. } => either::Either::Left(applied.rejected.iter()),
            Self::Failed { .. } => either::Either::Right(std::iter::empty()),
        }
    }

    pub fn is_success(&self) -> bool {
        match self {
            Self::Success { .. } => true,
            Self::Failed { .. } => false,
        }
    }
}

#[derive(Default)]
pub struct FetchState {
    /// In-memory refdb used to keep track of new updates without
    /// committing them to the real refdb until all validation has
    /// occurred.
    refs: git::mem::Refdb,
    /// Have we seen the `rad/id` reference?
    canonical_rad_id: Option<Oid>,
    /// Seen remote `rad/id` tips.
    ids: IdentityTips,
    /// Seen remote `rad/sigrefs` tips.
    sigrefs: SigrefTips,
    /// Seen reference tips, per remote.
    tips: BTreeMap<PublicKey, Vec<Update<'static>>>,
}

impl FetchState {
    /// Remove all tips associated with this `remote` in the
    /// `FetchState`.
    pub fn prune(&mut self, remote: &PublicKey) {
        self.ids.remove(remote);
        self.sigrefs.remove(remote);
        self.tips.remove(remote);
    }

    pub fn canonical_rad_id(&self) -> Option<&Oid> {
        self.canonical_rad_id.as_ref()
    }

    /// Update the in-memory refdb with the given updates while also
    /// keeping track of the updates in [`FetchState::tips`].
    pub fn update_all<'a, I>(&mut self, other: I) -> Applied<'a>
    where
        I: IntoIterator<Item = (PublicKey, Vec<Update<'a>>)>,
    {
        let mut ap = Applied::default();
        for (remote, ups) in other {
            for up in &ups {
                ap.append(&mut self.refs.update(Some(up.clone())));
            }
            let mut ups = ups
                .into_iter()
                .map(|up| up.into_owned())
                .collect::<Vec<_>>();
            self.tips
                .entry(remote)
                .and_modify(|tips| tips.append(&mut ups))
                .or_insert(ups);
        }
        ap
    }

    pub(crate) fn as_cached<'a, S>(&'a mut self, handle: &'a mut Handle<S>) -> Cached<'a, S> {
        Cached {
            handle,
            state: self,
        }
    }
}

impl FetchState {
    /// Perform the ls-refs and fetch for the given `step`. The result
    /// of these processes is kept track of in the internal state.
    pub(super) fn run_stage<S, F>(
        &mut self,
        handle: &mut Handle<S>,
        handshake: &handshake::Outcome,
        step: &F,
    ) -> Result<BTreeSet<PublicKey>, error::Step>
    where
        S: transport::ConnectionStream,
        F: ProtocolStage,
    {
        let refs = match step.ls_refs() {
            Some(refs) => handle
                .transport
                .ls_refs(refs.into(), handshake)?
                .into_iter()
                .filter_map(|r| step.ref_filter(r))
                .collect::<Vec<_>>(),
            None => vec![],
        };
        log::trace!(target: "fetch", "Received refs {:?}", refs);
        step.pre_validate(&refs)?;

        let wants_haves = step.wants_haves(&handle.repo, &refs)?;
        if !wants_haves.wants.is_empty() {
            handle
                .transport
                .fetch(wants_haves, handle.interrupt.clone(), handshake)?;
        } else {
            log::trace!(target: "fetch", "Nothing to fetch")
        };

        let mut fetched = BTreeSet::new();
        for r in &refs {
            match &r.name {
                refs::ReceivedRefname::Namespaced { remote, suffix } => {
                    fetched.insert(*remote);
                    if let Some(rad) = suffix.as_ref().left() {
                        match rad {
                            refs::Special::Id => {
                                self.ids.insert(*remote, r.tip);
                            }

                            refs::Special::SignedRefs => {
                                self.sigrefs.insert(*remote, r.tip);
                            }
                        }
                    }
                }
                refs::ReceivedRefname::RadId => self.canonical_rad_id = Some(r.tip),
            }
        }

        let up = step.prepare_updates(self, &handle.repo, &refs)?;
        self.update_all(up.tips);

        Ok(fetched)
    }

    /// Fetch the set of special refs, depending on `refs_at`.
    ///
    /// If `refs_at` is `Some`, then run the [`SigrefsAt`] stage,
    /// which specifically fetches `rad/sigrefs` which are listed in
    /// `refs_at`.
    ///
    /// If `refs_at` is `None`, then run the [`SpecialRefs`] stage,
    /// which fetches `rad/sigrefs` and `rad/id` from all tracked and
    /// delegate peers (scope dependent).
    ///
    /// The resulting [`sigrefs::RemoteRefs`] will be the set of
    /// `rad/sigrefs` of the fetched remotes.
    #[allow(clippy::too_many_arguments)]
    fn run_special_refs<S>(
        &mut self,
        handle: &mut Handle<S>,
        handshake: &handshake::Outcome,
        delegates: BTreeSet<PublicKey>,
        threshold: usize,
        limit: &FetchLimit,
        remote: PublicKey,
        refs_at: Option<Vec<RefsAt>>,
    ) -> Result<sigrefs::RemoteRefs, error::Protocol>
    where
        S: transport::ConnectionStream,
    {
        match refs_at {
            Some(refs_at) => {
                let sigrefs_at = stage::SigrefsAt {
                    remote,
                    delegates: delegates.clone(),
                    refs_at: refs_at.clone(),
                    blocked: handle.blocked.clone(),
                    limit: limit.special,
                };
                log::trace!(target: "fetch", "{sigrefs_at:?}");
                self.run_stage(handle, handshake, &sigrefs_at)?;
                let remotes = refs_at.iter().map(|r| &r.remote);

                let signed_refs = sigrefs::RemoteRefs::load(&self.as_cached(handle), remotes)?;
                Ok(signed_refs)
            }
            None => {
                let followed = handle.allowed();
                log::trace!(target: "fetch", "Followed nodes {:?}", followed);
                let special_refs = stage::SpecialRefs {
                    blocked: handle.blocked.clone(),
                    remote,
                    delegates: delegates.clone(),
                    followed,
                    threshold,
                    limit: limit.special,
                };
                log::trace!(target: "fetch", "{special_refs:?}");
                let fetched = self.run_stage(handle, handshake, &special_refs)?;

                let signed_refs = sigrefs::RemoteRefs::load(
                    &self.as_cached(handle),
                    fetched.iter().chain(delegates.iter()),
                )?;
                Ok(signed_refs)
            }
        }
    }

    /// The finalization of the protocol exchange is as follows:
    ///
    ///   1. Load the canonical `rad/id` to use as the anchor for
    ///      getting the delegates of the identity.
    ///   2. Calculate the trusted set of peers for fetching from.
    ///   3. Fetch the special references, i.e. `rad/id` and `rad/sigrefs`.
    ///   4. Load the signed references, where these signed references
    ///      must be cryptographically verified for delegates,
    ///      otherwise they are discarded for non-delegates.
    ///   5. Fetch the data references, i.e. references found in
    ///      `rad/sigrefs`.
    ///   6. Validate the fetched references for delegates and
    ///      non-delegates, pruning any invalid remotes from the set
    ///      of updating tips.
    ///   7. Apply the valid tips, iff no delegates failed validation.
    ///   8. Signal to the other side that the process has completed.
    pub(super) fn run<S>(
        mut self,
        handle: &mut Handle<S>,
        handshake: &handshake::Outcome,
        limit: FetchLimit,
        remote: PublicKey,
        refs_at: Option<Vec<RefsAt>>,
    ) -> Result<FetchResult, error::Protocol>
    where
        S: transport::ConnectionStream,
    {
        let start = Instant::now();
        // N.b. we always fetch the `rad/id` since our delegate set
        // might be further ahead than theirs, e.g. we are the
        // deciding vote on adding a delegate.
        self.run_stage(
            handle,
            handshake,
            &stage::CanonicalId {
                remote,
                limit: limit.special,
            },
        )?;
        log::debug!(target: "fetch", "Fetched rad/id ({}ms)", start.elapsed().as_millis());

        // N.b. The error case here should not happen. In the case of
        // a `clone` we have asked for refs/rad/id and ensured it was
        // fetched. In the case of `pull` the repository should have
        // the refs/rad/id set.
        let anchor = self
            .as_cached(handle)
            .canonical()?
            .ok_or(error::Protocol::MissingRadId)?;

        let is_delegate = anchor.delegates.contains(&Did::from(handle.local()));
        // TODO: not sure we should allow to block *any* peer from the
        // delegate set. We could end up ignoring delegates.
        let delegates = anchor
            .delegates
            .iter()
            .filter(|id| !handle.is_blocked(id))
            .map(|did| PublicKey::from(*did))
            .collect::<BTreeSet<_>>();

        log::trace!(target: "fetch", "Identity delegates {delegates:?}");

        // The local peer does not need to count towards the threshold
        // since they must be valid already.
        let threshold = if is_delegate {
            anchor.threshold - 1
        } else {
            anchor.threshold
        };
        let signed_refs = self.run_special_refs(
            handle,
            handshake,
            delegates.clone(),
            threshold,
            &limit,
            remote,
            refs_at,
        )?;
        log::debug!(
            target: "fetch",
            "Fetched data for {} remote(s) ({}ms)",
            signed_refs.len(),
            start.elapsed().as_millis()
        );

        let data_refs = stage::DataRefs {
            remote,
            remotes: signed_refs,
            limit: limit.refs,
        };
        self.run_stage(handle, handshake, &data_refs)?;
        log::debug!(
            target: "fetch",
            "Fetched data refs for {} remotes ({}ms)",
            data_refs.remotes.len(),
            start.elapsed().as_millis()
        );

        // N.b. signal to exit the upload-pack sequence
        // We're finished fetching on this side, and all that's left
        // is validation.
        match handle.transport.done() {
            Ok(()) => log::debug!(target: "fetch", "Sent done signal to remote {remote}"),
            Err(err) => {
                log::warn!(target: "fetch", "Attempted to send done to remote {remote}: {err}")
            }
        }

        // Run validation of signed refs, pruning any offending
        // remotes from the tips, thus not updating the production Git
        // repository.
        let mut failures = sigrefs::Validations::default();
        let signed_refs = data_refs.remotes;

        // We may prune fetched remotes, so we keep track of
        // non-pruned, fetched remotes here.
        let mut remotes = BTreeSet::new();

        // The valid delegates start with all delegates that this peer
        // currently has valid references for
        let mut valid_delegates = handle
            .repository()
            .remote_ids()
            .map_err(error::Protocol::RemoteIds)?
            .filter_map(|id| id.ok())
            .filter(|id| delegates.contains(id))
            .collect::<BTreeSet<_>>();
        let mut failed_delegates = BTreeSet::new();

        // TODO(finto): this might read better if it got its own
        // private function.
        for remote in signed_refs.keys() {
            if handle.is_blocked(remote) {
                log::trace!(target: "fetch", "Skipping blocked remote {remote}");
                continue;
            }

            let remote = sigrefs::DelegateStatus::empty(*remote, &delegates)
                .load(&self.as_cached(handle))?;
            match remote {
                sigrefs::DelegateStatus::NonDelegate { remote, data: None } => {
                    log::debug!(target: "fetch", "Pruning non-delegate {remote} tips, missing 'rad/sigrefs'");
                    failures.push(sigrefs::Validation::MissingRadSigRefs(remote));
                    self.prune(&remote);
                }
                sigrefs::DelegateStatus::Delegate { remote, data: None } => {
                    log::warn!(target: "fetch", "Pruning delegate {remote} tips, missing 'rad/sigrefs'");
                    failures.push(sigrefs::Validation::MissingRadSigRefs(remote));
                    self.prune(&remote);
                    // This delegate has removed their `rad/sigrefs`.
                    // Technically, we can continue with their
                    // previous `rad/sigrefs` but if this occurs with
                    // enough delegates also failing validation we
                    // would rather surface the issue and fail the fetch.
                    valid_delegates.remove(&remote);
                    failed_delegates.insert(remote);
                }
                sigrefs::DelegateStatus::NonDelegate {
                    remote,
                    data: Some(sigrefs),
                } => {
                    if let Some(SignedRefsAt { at, .. }) = SignedRefsAt::load(remote, &handle.repo)?
                    {
                        // Prune non-delegates if they're behind or
                        // diverged. A diverged case is non-fatal for
                        // delegates.
                        if matches!(
                            repository::ancestry(&handle.repo, at, sigrefs.at)?,
                            repository::Ancestry::Behind | repository::Ancestry::Diverged
                        ) {
                            self.prune(&remote);
                            continue;
                        }
                    }

                    let cache = self.as_cached(handle);
                    if let Some(warns) = sigrefs::validate(&cache, sigrefs)?.as_mut() {
                        log::debug!(
                            target: "fetch",
                            "Pruning non-delegate {remote} tips, due to validation failures"
                        );
                        self.prune(&remote);
                        failures.append(warns);
                    } else {
                        remotes.insert(remote);
                    }
                }
                sigrefs::DelegateStatus::Delegate {
                    remote,
                    data: Some(sigrefs),
                } => {
                    if let Some(SignedRefsAt { at, .. }) = SignedRefsAt::load(remote, &handle.repo)?
                    {
                        let ancestry = repository::ancestry(&handle.repo, at, sigrefs.at)?;
                        if matches!(ancestry, repository::Ancestry::Behind) {
                            log::trace!(target: "fetch", "Advertised `rad/sigrefs` {} is behind {at} for {remote}", sigrefs.at);
                            self.prune(&remote);
                            continue;
                        } else if matches!(ancestry, repository::Ancestry::Diverged) {
                            return Err(error::Protocol::Diverged {
                                remote,
                                current: at,
                                received: sigrefs.at,
                            });
                        }
                    }

                    let cache = self.as_cached(handle);
                    let mut fails = Validations::default();
                    // N.b. we only validate the existence of the
                    // default branch for delegates, since it safe for
                    // non-delegates to not have this branch.
                    let branch_validation =
                        validate_project_default_branch(&anchor, &sigrefs.sigrefs);
                    fails.extend(branch_validation.into_iter());
                    let validations = sigrefs::validate(&cache, sigrefs)?;
                    fails.extend(validations.into_iter().flatten());
                    if !fails.is_empty() {
                        log::warn!(target: "fetch", "Pruning delegate {remote} tips, due to validation failures");
                        self.prune(&remote);
                        valid_delegates.remove(&remote);
                        failed_delegates.insert(remote);
                        failures.append(&mut fails)
                    } else {
                        valid_delegates.insert(remote);
                        remotes.insert(remote);
                    }
                }
            }
        }
        log::debug!(
            target: "fetch",
            "Validated {} remote(s) ({}ms)",
            remotes.len(),
            start.elapsed().as_millis()
        );

        // N.b. only apply to Git repository if there are enough valid
        // delegates that pass the threshold.
        if valid_delegates.len() >= threshold {
            let applied = repository::update(
                &handle.repo,
                self.tips
                    .clone()
                    .into_values()
                    .flat_map(|ups| ups.into_iter()),
            )?;
            log::debug!(target: "fetch", "Applied updates ({}ms)", start.elapsed().as_millis());
            Ok(FetchResult::Success {
                applied,
                remotes,
                validations: failures,
            })
        } else {
            log::debug!(
                target: "fetch",
                "Fetch failed: {} failure(s) ({}ms)",
                failures.len(),
                start.elapsed().as_millis()
            );
            Ok(FetchResult::Failed {
                threshold,
                delegates: failed_delegates,
                validations: failures,
            })
        }
    }
}

/// A cached version of [`Handle`] by using the underlying
/// [`FetchState`]'s data for performing lookups.
pub(crate) struct Cached<'a, S> {
    handle: &'a mut Handle<S>,
    state: &'a mut FetchState,
}

impl<'a, S> Cached<'a, S> {
    /// Resolves `refname` to its [`ObjectId`] by first looking at the
    /// [`FetchState`] and falling back to the [`Handle::refdb`].
    pub fn refname_to_id<'b, N>(
        &self,
        refname: N,
    ) -> Result<Option<Oid>, repository::error::Resolve>
    where
        N: Into<Qualified<'b>>,
    {
        let refname = refname.into();
        match self.state.refs.refname_to_id(refname.clone()) {
            None => repository::refname_to_id(&self.handle.repo, refname),
            Some(oid) => Ok(Some(oid)),
        }
    }

    /// Get the `rad/id` found in the [`FetchState`].
    pub fn canonical_rad_id(&self) -> Option<Oid> {
        self.state.canonical_rad_id().copied()
    }

    pub fn verified(&self, head: Oid) -> Result<Doc<Verified>, DocError> {
        self.handle.verified(head)
    }

    pub fn canonical(&self) -> Result<Option<Doc<Verified>>, error::Canonical> {
        let tip = self.refname_to_id(refs::REFS_RAD_ID.clone())?;
        let cached_tip = self.canonical_rad_id();

        tip.or(cached_tip)
            .map(|tip| self.verified(tip).map_err(error::Canonical::from))
            .transpose()
    }

    pub fn load(&self, remote: &PublicKey) -> Result<Option<SignedRefsAt>, sigrefs::error::Load> {
        match self.state.sigrefs.get(remote) {
            None => SignedRefsAt::load(*remote, &self.handle.repo),
            Some(tip) => SignedRefsAt::load_at(*tip, *remote, &self.handle.repo).map(Some),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn inspect(&self) {
        self.state.refs.inspect()
    }
}

impl<'a, S> RemoteRepository for Cached<'a, S> {
    fn remote(&self, remote: &RemoteId) -> Result<Remote, storage::refs::Error> {
        // N.b. this is unused so we just delegate to the underlying
        // repository for a correct implementation.
        self.handle.repo.remote(remote)
    }

    fn remotes(&self) -> Result<Remotes<Verified>, storage::refs::Error> {
        self.state
            .sigrefs
            .keys()
            .map(|id| self.remote(id).map(|remote| (*id, remote)))
            .collect::<Result<_, _>>()
    }

    fn remote_refs_at(&self) -> Result<Vec<RefsAt>, storage::refs::Error> {
        self.handle.repo.remote_refs_at()
    }
}

impl<'a, S> ValidateRepository for Cached<'a, S> {
    // N.b. we don't verify the `rad/id` of each remote since they may
    // not have a reference to the COB if they have not interacted
    // with it.
    fn validate_remote(&self, remote: &Remote) -> Result<Validations, storage::Error> {
        // Contains a copy of the signed refs of this remote.
        let mut signed = BTreeMap::from((*remote.refs).clone());
        let mut validations = Validations::default();
        let mut has_sigrefs = false;

        // Check all repository references, making sure they are present in the signed refs map.
        for (refname, oid) in self.state.refs.references_of(&remote.id) {
            // Skip validation of the signed refs branch, as it is not part of `Remote`.
            if refname == storage::refs::SIGREFS_BRANCH.to_ref_string() {
                has_sigrefs = true;
                continue;
            }
            if let Some(signed_oid) = signed.remove(&refname) {
                if oid != signed_oid {
                    validations.push(Validation::MismatchedRef {
                        refname,
                        expected: signed_oid,
                        actual: oid,
                    });
                }
            } else {
                validations.push(Validation::UnsignedRef(refname));
            }
        }

        if !has_sigrefs {
            validations.push(Validation::MissingRadSigRefs(remote.id));
        }

        // The refs that are left in the map, are ones that were signed, but are not
        // in the repository. If any are left, bail.
        for (name, _) in signed.into_iter() {
            validations.push(Validation::MissingRef {
                refname: name,
                remote: remote.id,
            });
        }

        Ok(validations)
    }
}

/// If the repository has a project payload, in `anchor`, then
/// validate that the `sigrefs` contains the listed default branch.
///
/// N.b. if the repository does not have the project payload or a
/// deserialization error occurs, then this will return `None`.
fn validate_project_default_branch(
    anchor: &Doc<Verified>,
    sigrefs: &SignedRefs<Verified>,
) -> Option<Validation> {
    let proj = anchor.project().ok()?;
    let branch = radicle::git::refs::branch(proj.default_branch()).to_ref_string();
    (!sigrefs.contains_key(&branch)).then_some(Validation::MissingRef {
        remote: sigrefs.id,
        refname: branch,
    })
}

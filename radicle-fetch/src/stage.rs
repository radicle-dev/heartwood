//! The Radicle fetch protocol can be split into two actions: `clone`
//! and `pull`. Each of these actions will interact with the server in
//! multiple stages, where each stage will perform a single roundtrip
//! of fetching. These stages are encapsulated in the
//! [`ProtocolStage`] trait.
//!
//! ### Clone
//!
//! A `clone` is split into three stages:
//!
//!   1. [`CanonicalId`]: fetches the canonical `refs/rad/id` to use
//!      as an anchor for the rest of the fetch, i.e. provides initial
//!      delegate data for the repository.
//!   2. [`SpecialRefs`]: fetches the special references, `rad/id` and
//!      `rad/sigrefs`, for each configured namespace, i.e. tracked
//!      and delegate peers if the scope is trusted and all peers is the
//!      scope is all.
//!   3. [`DataRefs`]: fetches the `Oid`s for each reference listed in
//!      the `rad/sigrefs` for each fetched peer in the
//!      [`SpecialRefs`] stage. Additionally, any references that have
//!      been removed from `rad/sigrefs` are marked for deletion.
//!
//! ### Pull
//!
//! A `pull` is split into two stages:
//!
//!   1. [`SpecialRefs`]: see above.
//!   2. [`DataRefs`]: see above.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use bstr::BString;
use either::Either;
use gix_protocol::handshake::Ref;
use nonempty::NonEmpty;
use radicle::crypto::PublicKey;
use radicle::git::{refname, Component, Namespaced, Qualified};
use radicle::storage::git::Repository;
use radicle::storage::refs::{RefsAt, Special};
use radicle::storage::ReadRepository;

use crate::git::refs::{Policy, Update, Updates};
use crate::refs::{ReceivedRef, ReceivedRefname};
use crate::sigrefs;
use crate::state::FetchState;
use crate::tracking::BlockList;
use crate::transport::WantsHaves;
use crate::{refs, tracking};

pub mod error {
    use radicle::crypto::PublicKey;
    use radicle::git::RefString;
    use thiserror::Error;

    use crate::transport::WantsHavesError;

    #[derive(Debug, Error)]
    pub enum Layout {
        #[error("missing required refs: {0:?}")]
        MissingRequiredRefs(Vec<String>),
    }

    #[derive(Debug, Error)]
    pub enum Prepare {
        #[error(transparent)]
        References(#[from] radicle::storage::Error),
        #[error("verification of rad/id for {remote} failed")]
        Verification {
            remote: PublicKey,
            #[source]
            err: Box<dyn std::error::Error + Send + Sync + 'static>,
        },
    }

    #[derive(Debug, Error)]
    pub enum WantsHaves {
        #[error(transparent)]
        WantsHavesAdd(#[from] WantsHavesError),
        #[error("expected namespaced ref {0}")]
        NotNamespaced(RefString),
    }
}

/// A [`ProtocolStage`] describes a single roundtrip with the Radicle
/// node that is serving the data.
///
/// The stages are used as input for [`crate::FetchState::step`] and
/// are called in the order that they are listed here, .i.e:
///
///   1. `ls_refs`: asks the server for the provided reference
///       prefixes.
///   2. `ref_filter`: filter the advertised refs to the set required
///       for inspection.
///   3. `pre_validate`: before fetching the data, ensure the server
///       advertised the references that are required.
///   4. `wants_haves`: build the set of `want`s and `have`s to send
///       to the server.
///   5. `prepare_updates`: prepares the set of updates to update the
///      refdb (in-memory and production).
pub(crate) trait ProtocolStage {
    /// If and how to perform `ls-refs`.
    fn ls_refs(&self) -> Option<NonEmpty<BString>>;

    /// Filter a remote-advertised [`Ref`].
    ///
    /// Return `Some` if the ref should be considered, `None` otherwise. This
    /// method may be called with the response of `ls-refs`, the `wanted-refs`
    /// of a `fetch` response, or both.
    fn ref_filter(&self, r: Ref) -> Option<ReceivedRef>;

    /// Validate that all advertised refs conform to an expected layout.
    ///
    /// The supplied `refs` are `ls-ref`-advertised refs filtered
    /// through [`ProtocolStage::ref_filter`].
    fn pre_validate(&self, refs: &[ReceivedRef]) -> Result<(), error::Layout>;

    /// Assemble the `want`s and `have`s for a `fetch`, retaining the refs which
    /// would need updating after the `fetch` succeeds.
    ///
    /// The `refs` are the advertised refs from executing `ls-refs`, filtered
    /// through [`ProtocolStage::ref_filter`].
    fn wants_haves(
        &self,
        refdb: &Repository,
        refs: &[ReceivedRef],
    ) -> Result<WantsHaves, error::WantsHaves> {
        let mut wants_haves = WantsHaves::default();
        wants_haves.add(
            refdb,
            refs.iter().map(|recv| (recv.to_qualified(), recv.tip)),
        )?;
        Ok(wants_haves)
    }

    /// Prepare the [`Updates`] based on the received `refs`.
    ///
    /// These updates can then be used to update the refdb.
    fn prepare_updates<'a>(
        &self,
        s: &FetchState,
        repo: &Repository,
        refs: &'a [ReceivedRef],
    ) -> Result<Updates<'a>, error::Prepare>;
}

/// The [`ProtocolStage`] for performing an initial clone from a `remote`.
///
/// This step asks for the canonical `refs/rad/id` reference, which
/// allows us to use it as an anchor for the following steps.
#[derive(Debug)]
pub struct CanonicalId {
    pub remote: PublicKey,
    pub limit: u64,
}

impl ProtocolStage for CanonicalId {
    fn ls_refs(&self) -> Option<NonEmpty<BString>> {
        Some(NonEmpty::new(refs::REFS_RAD_ID.as_bstr().into()))
    }

    fn ref_filter(&self, r: Ref) -> Option<ReceivedRef> {
        match refs::unpack_ref(r).ok()? {
            (
                refname @ ReceivedRefname::Namespaced {
                    suffix: Either::Left(_),
                    ..
                },
                tip,
            ) => Some(ReceivedRef::new(tip, refname)),
            (ReceivedRefname::RadId, tip) => Some(ReceivedRef::new(tip, ReceivedRefname::RadId)),
            _ => None,
        }
    }

    fn pre_validate(&self, refs: &[ReceivedRef]) -> Result<(), error::Layout> {
        // Ensures that we fetched the canonical 'refs/rad/id'
        ensure_refs(
            [BString::from(refs::REFS_RAD_ID.as_bstr())]
                .into_iter()
                .collect(),
            refs.iter()
                .map(|r| r.to_qualified().to_string().into())
                .collect(),
        )
    }

    fn prepare_updates<'a>(
        &self,
        s: &FetchState,
        repo: &Repository,
        refs: &'a [ReceivedRef],
    ) -> Result<Updates<'a>, error::Prepare> {
        // SAFETY: checked by `pre_validate` that the `refs/rad/id`
        // was received
        let verified = repo
            .identity_doc_at(
                *s.canonical_rad_id()
                    .expect("ensure we got canonicdal 'rad/id' ref"),
            )
            .map_err(|err| error::Prepare::Verification {
                remote: self.remote,
                err: Box::new(err),
            })?;
        if verified.delegates.contains(&self.remote.into()) {
            let is_delegate = |remote: &PublicKey| verified.is_delegate(remote);
            Ok(Updates::build(
                refs.iter()
                    .filter_map(|r| r.as_special_ref_update(is_delegate)),
            ))
        } else {
            Ok(Updates::default())
        }
    }
}

/// The [`ProtocolStage`] for fetching special refs from the set of
/// remotes in `tracked` and `delegates`.
///
/// This step asks for all tracked and delegate remote's `rad/id` and
/// `rad/sigrefs`, iff the scope is
/// [`tracking::Scope::Trusted`]. Otherwise, it asks for all
/// namespaces.
///
/// It ensures that all delegate refs were fetched.
#[derive(Debug)]
pub struct SpecialRefs {
    /// The set of nodes that should be blocked from fetching.
    pub blocked: BlockList,
    /// The node that is being fetched from.
    pub remote: PublicKey,
    /// The set of nodes to be fetched.
    pub tracked: tracking::Tracked,
    /// The set of delegates to be fetched, with the local node
    /// removed in the case of a `pull`.
    pub delegates: BTreeSet<PublicKey>,
    /// The data limit for this stage of fetching.
    pub limit: u64,
}

impl ProtocolStage for SpecialRefs {
    fn ls_refs(&self) -> Option<NonEmpty<BString>> {
        match &self.tracked {
            tracking::Tracked::All => Some(NonEmpty::new("refs/namespaces".into())),
            tracking::Tracked::Trusted { remotes } => NonEmpty::collect(
                remotes
                    .iter()
                    .chain(self.delegates.iter())
                    .flat_map(|remote| {
                        [
                            BString::from(radicle::git::refs::storage::id(remote).to_string()),
                            BString::from(radicle::git::refs::storage::sigrefs(remote).to_string()),
                        ]
                    }),
            ),
        }
    }

    fn ref_filter(&self, r: Ref) -> Option<ReceivedRef> {
        let (refname, tip) = refs::unpack_ref(r).ok()?;
        match refname {
            // N.b. ensure that any blocked peers are filtered since
            // `Scope::All` can ls for them
            ReceivedRefname::Namespaced { remote, .. } if self.blocked.is_blocked(&remote) => None,
            ReceivedRefname::Namespaced { ref suffix, .. } if suffix.is_left() => {
                Some(ReceivedRef::new(tip, refname))
            }
            ReceivedRefname::Namespaced { .. } | ReceivedRefname::RadId => None,
        }
    }

    fn pre_validate(&self, refs: &[ReceivedRef]) -> Result<(), error::Layout> {
        ensure_refs(
            self.delegates
                .iter()
                .filter(|id| !self.blocked.is_blocked(id))
                .map(|id| {
                    // N.b. we asked for the rad/id but do not need to ensure it
                    BString::from(radicle::git::refs::storage::sigrefs(id).to_string())
                })
                .collect(),
            refs.iter()
                .filter_map(|r| r.name.to_namespaced())
                .map(|r| r.to_string().into())
                .collect(),
        )
    }

    fn prepare_updates<'a>(
        &self,
        _s: &FetchState,
        _repo: &Repository,
        refs: &'a [ReceivedRef],
    ) -> Result<Updates<'a>, error::Prepare> {
        special_refs_updates(&self.delegates, &self.blocked, refs)
    }
}

/// The [`ProtocolStage`] for fetching announce `rad/sigrefs`.
///
/// This step will ask for the `rad/sigrefs` for the remotes of
/// `refs_at`.
#[derive(Debug)]
pub struct SigrefsAt {
    /// The set of nodes that should be blocked from fetching.
    pub blocked: BlockList,
    /// The node that is being fetched from.
    pub remote: PublicKey,
    /// The set of remotes and the newly announced `Oid` for their
    /// `rad/sigrefs`.
    pub refs_at: Vec<RefsAt>,
    /// The set of delegates to be fetched, with the local node
    /// removed in the case of a `pull`.
    pub delegates: BTreeSet<PublicKey>,
    /// The data limit for this stage of fetching.
    pub limit: u64,
}

impl ProtocolStage for SigrefsAt {
    fn ls_refs(&self) -> Option<NonEmpty<BString>> {
        // N.b. the `Oid`s are known but the `rad/sigrefs` are still
        // asked for to mark them for updating the fetch state.
        NonEmpty::collect(self.refs_at.iter().map(|refs_at| {
            BString::from(radicle::git::refs::storage::sigrefs(&refs_at.remote).to_string())
        }))
    }

    // We only asked for `rad/sigrefs` so we should only get
    // `rad/sigrefs`.
    fn ref_filter(&self, r: Ref) -> Option<ReceivedRef> {
        let (refname, tip) = refs::unpack_ref(r).ok()?;
        match refname {
            ReceivedRefname::Namespaced { remote, .. } if self.blocked.is_blocked(&remote) => None,
            ReceivedRefname::Namespaced {
                suffix: Either::Left(Special::SignedRefs),
                ..
            } => Some(ReceivedRef::new(tip, refname)),
            ReceivedRefname::Namespaced { .. } | ReceivedRefname::RadId => None,
        }
    }

    fn pre_validate(&self, _refs: &[ReceivedRef]) -> Result<(), error::Layout> {
        Ok(())
    }

    fn wants_haves(
        &self,
        refdb: &Repository,
        refs: &[ReceivedRef],
    ) -> Result<WantsHaves, error::WantsHaves> {
        let mut wants_haves = WantsHaves::default();
        let sigrefs = self
            .refs_at
            .iter()
            .map(|RefsAt { remote, at }| (Special::SignedRefs.namespaced(remote), *at));
        wants_haves.add(refdb, sigrefs)?;
        wants_haves.add(
            refdb,
            refs.iter().map(|recv| (recv.to_qualified(), recv.tip)),
        )?;
        Ok(wants_haves)
    }

    fn prepare_updates<'a>(
        &self,
        _s: &FetchState,
        _repo: &Repository,
        _refs: &'a [ReceivedRef],
    ) -> Result<Updates<'a>, error::Prepare> {
        let mut updates = Updates::default();
        for RefsAt { remote, at } in self.refs_at.iter() {
            if let Some(up) =
                refs::special_update(remote, &Either::Left(Special::SignedRefs), *at, |remote| {
                    self.delegates.contains(remote)
                })
            {
                updates.add(*remote, up);
            }
        }
        Ok(updates)
    }
}

/// The [`ProtocolStage`] for fetching data refs from the set of
/// remotes in `trusted`.
///
/// All refs that are listed in the `remotes` sigrefs are checked
/// against our refdb/odb to build a set of `wants` and `haves`. The
/// `wants` will then be fetched from the server side to receive those
/// particular objects.
///
/// Those refs and objects are then prepared for updating, removing
/// any that were found to exist before the latest fetch.
#[derive(Debug)]
pub struct DataRefs {
    /// The node that is being fetched from.
    pub remote: PublicKey,
    /// The set of signed references from each remote that was
    /// fetched.
    pub remotes: sigrefs::RemoteRefs,
    /// The data limit for this stage of fetching.
    pub limit: u64,
}

impl ProtocolStage for DataRefs {
    // We don't need to ask for refs since we have all reference names
    // and `Oid`s in `rad/sigrefs`.
    fn ls_refs(&self) -> Option<NonEmpty<BString>> {
        None
    }

    // Since we don't ask for refs, we don't need to filter them.
    fn ref_filter(&self, _: Ref) -> Option<ReceivedRef> {
        None
    }

    // Since we don't ask for refs, we don't need to validate them.
    fn pre_validate(&self, _refs: &[ReceivedRef]) -> Result<(), error::Layout> {
        Ok(())
    }

    // We ignore the `ReceivedRef`s since we are using the `remotes`
    // as the source for refnames and `Oid`s.
    fn wants_haves(
        &self,
        refdb: &Repository,
        _refs: &[ReceivedRef],
    ) -> Result<WantsHaves, error::WantsHaves> {
        let mut wants_haves = WantsHaves::default();

        for (remote, loaded) in &self.remotes {
            wants_haves.add(
                refdb,
                loaded.refs.iter().filter_map(|(refname, tip)| {
                    let refname = Qualified::from_refstr(refname)
                        .map(|refname| refname.with_namespace(Component::from(remote)))?;
                    Some((refname, *tip))
                }),
            )?;
        }

        Ok(wants_haves)
    }

    fn prepare_updates<'a>(
        &self,
        _s: &FetchState,
        repo: &Repository,
        _refs: &'a [ReceivedRef],
    ) -> Result<Updates<'a>, error::Prepare> {
        let mut updates = Updates::default();

        for (remote, refs) in &self.remotes {
            let mut signed = HashSet::with_capacity(refs.refs.len());
            for (name, tip) in refs.iter() {
                let tracking: Namespaced<'_> = Qualified::from_refstr(name)
                    .and_then(|q| refs::ReceivedRefname::remote(*remote, q).to_namespaced())
                    .expect("we checked sigrefs well-formedness in wants_refs already");
                signed.insert(tracking.clone());
                updates.add(
                    *remote,
                    Update::Direct {
                        name: tracking,
                        target: *tip,
                        no_ff: Policy::Allow,
                    },
                );
            }

            // Prune refs not in signed
            let prefix_rad = refname!("refs/rad");
            for (name, target) in repo.references_of(remote)? {
                // 'rad/' refs are never subject to pruning
                if name.starts_with(prefix_rad.as_str()) {
                    continue;
                }

                let name = Qualified::from_refstr(name)
                    .expect("BUG: reference is guaranteed to be Qualified")
                    .with_namespace(Component::from(remote));

                if !signed.contains(&name) {
                    updates.add(
                        *remote,
                        Update::Prune {
                            name,
                            prev: either::Left(target),
                        },
                    );
                }
            }
        }

        Ok(updates)
    }
}

// N.b. the `delegates` are the delegates of the repository, with the
// potential removal of the local peer in the case of a `pull`.
fn special_refs_updates<'a>(
    delegates: &BTreeSet<PublicKey>,
    blocked: &BlockList,
    refs: &'a [ReceivedRef],
) -> Result<Updates<'a>, error::Prepare> {
    use either::Either::*;

    let grouped = refs
        .iter()
        .filter_map(|r| match &r.name {
            refs::ReceivedRefname::Namespaced { remote, suffix } => {
                (!blocked.is_blocked(remote)).then_some((remote, r.tip, suffix.clone()))
            }
            refs::ReceivedRefname::RadId => None,
        })
        .fold(
            BTreeMap::<PublicKey, Vec<_>>::new(),
            |mut acc, (remote_id, tip, name)| {
                acc.entry(*remote_id).or_default().push((tip, name));
                acc
            },
        );

    let mut updates = Updates::default();

    for (remote_id, refs) in grouped {
        let mut tips_inner = Vec::with_capacity(2);
        for (tip, suffix) in &refs {
            match &suffix {
                Left(refs::Special::Id) => {
                    if let Some(u) = refs::special_update(&remote_id, suffix, *tip, |remote| {
                        delegates.contains(remote)
                    }) {
                        tips_inner.push(u);
                    }
                }

                Left(refs::Special::SignedRefs) => {
                    if let Some(u) = refs::special_update(&remote_id, suffix, *tip, |remote| {
                        delegates.contains(remote)
                    }) {
                        tips_inner.push(u);
                    }
                }

                Right(_) => continue,
            }
        }

        updates.append(remote_id, tips_inner);
    }

    Ok(updates)
}

fn ensure_refs<T>(required: BTreeSet<T>, wants: BTreeSet<T>) -> Result<(), error::Layout>
where
    T: Ord + ToString,
{
    if wants.is_empty() {
        return Ok(());
    }

    let diff = required.difference(&wants).collect::<Vec<_>>();

    if diff.is_empty() {
        Ok(())
    } else {
        Err(error::Layout::MissingRequiredRefs(
            diff.into_iter().map(|ns| ns.to_string()).collect(),
        ))
    }
}

use qcheck::{Arbitrary, QuickCheck, TestResult};

use crate::assert_matches;
use crate::cob;
use crate::cob::identity::{Did, Identity, RedactedBy, RejectedBy, RevisionId, State};
use crate::cob::store::Transaction;
use crate::identity::doc::PayloadId;
use crate::test::arbitrary::BoundedVec;
use crate::test::setup::Network;

// NOTE: If these tests take too much time to run, consider lowering the
// second argument (`const N: usize`) of the `ops: BoundedVec` argument.
// Additionally you can reduce `.tests(X)` or set `QUICKCHECK_TESTS=X`
// environment variable to a lower number, which reduces the number of
// passing tests quickcheck requires before returning green.
#[test]
fn prop_invariants() {
    fn prop(ops: BoundedVec<TestOp, 8>) -> TestResult {
        if ops.is_empty() {
            return TestResult::discard();
        }

        let mut harness = Harness::new();
        for op in ops {
            harness.apply(&op);
        }

        harness.assert_local_invariants();
        harness.converge();
        harness.assert_local_invariants();
        harness.assert_convergence();

        TestResult::passed()
    }

    QuickCheck::new()
        .tests(10)
        .quickcheck(prop as fn(BoundedVec<TestOp, 8>) -> TestResult);
}

/// The property tests keep track of 3 actors interacting with the repository
/// identity.
#[derive(Clone, Copy, Debug)]
enum Actor {
    Alice,
    Bob,
    Eve,
    Dave,
}

/// Enumerate all the actors in an array.
const ACTORS: [Actor; 4] = [Actor::Alice, Actor::Bob, Actor::Eve, Actor::Dave];

/// [`OpAction`] describes a given action that can be made when emulating
/// interactions with the repository identity document.
#[derive(Clone, Debug)]
enum OpAction {
    /// The repository identity is updated with a new description and given
    /// [`Actor`]'s delegate status is updated in the identity document.
    ///
    /// If they were previously a delegate then their delegate status is
    /// rescinded.
    ///
    /// Otherwise, they are added as a delegate.
    Update { toggle_delegate: Actor },
    /// The repository identity is updated with a new description and given
    /// [`Actor`]'s delegate status is updated in the identity document.
    /// The given `parent_idx` chooses a random revision to use as the parent.
    /// This ensures that sibling operations are created.
    ///
    /// If they were previously a delegate then their delegate status is
    /// rescinded.
    ///
    /// Otherwise, they are added as a delegate.
    UpdateRandom {
        toggle_delegate: Actor,
        parent_idx: usize,
    },
    /// The revision found at the given index is accepted.
    Accept(usize),
    /// The revision found at the given index is rejected.
    Reject(usize),
    /// The revision found at the given index is redacted.
    Redact(usize),
    /// The [`Actor`] paired with this operation synchronizes with all other
    /// actors.
    Sync,
}

/// Combine an [`OpAction`] with an [`Actor`], where the [`Actor`] performs the
/// action on the repository identity.
#[derive(Clone, Debug)]
struct TestOp {
    actor: Actor,
    action: OpAction,
}

impl Arbitrary for TestOp {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let actor = *g.choose(&ACTORS).unwrap();

        let action = match u8::arbitrary(g) % 6 {
            0 => OpAction::Update {
                toggle_delegate: *g.choose(&ACTORS).unwrap(),
            },
            1 => OpAction::UpdateRandom {
                toggle_delegate: *g.choose(&ACTORS).unwrap(),
                parent_idx: usize::arbitrary(g),
            },
            2 => OpAction::Accept(usize::arbitrary(g)),
            3 => OpAction::Reject(usize::arbitrary(g)),
            4 => OpAction::Redact(usize::arbitrary(g)),
            _ => OpAction::Sync,
        };

        Self { actor, action }
    }
}

/// A test harness that contains a [`Network`] of nodes that interact with a
/// shared repository [`Identity`].
///
/// These interactions are then verified to hold against a set of properties
/// expected of the [`Identity`].
struct Harness {
    network: Network,
    revisions: Vec<RevisionId>,
}

impl Harness {
    fn new() -> Self {
        let network = Network::default();
        let alice = &network.alice;
        let bob = &network.bob;
        let eve = &network.eve;

        let mut alice_identity = Identity::load_mut(&*alice.repo, &alice.signer).unwrap();
        let mut alice_doc = alice_identity.doc().clone().edit();
        alice_doc.delegate(bob.signer.public_key().into());
        alice_doc.delegate(eve.signer.public_key().into());
        alice_doc.threshold = 2;

        let a1 = alice_identity
            .update(
                cob::Title::new("Init").unwrap(),
                "",
                &alice_doc.verified().unwrap(),
            )
            .unwrap();

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);

        let mut bob_identity = Identity::load_mut(&*bob.repo, &bob.signer).unwrap();
        let mut eve_identity = Identity::load_mut(&*eve.repo, &eve.signer).unwrap();

        bob_identity.accept(&a1).unwrap();
        eve_identity.accept(&a1).unwrap();

        alice.repo.fetch(bob);
        alice.repo.fetch(eve);

        Self {
            network,
            revisions: vec![a1],
        }
    }

    /// The parent document always dictates the active delegate set
    fn is_delegate(&self, actor: Actor, parent_id: RevisionId) -> bool {
        let identity = self.identity_for(&actor);
        let parent_doc = &identity.revision(&parent_id).unwrap().doc;

        parent_doc.is_delegate(&self.did_for(&actor))
    }

    fn has_apply_error<E, F>(err: &E, predicate: F) -> bool
    where
        E: std::error::Error + 'static,
        F: Fn(&cob::identity::ApplyError) -> bool,
    {
        std::iter::successors(Some(err as &dyn std::error::Error), |e| e.source())
            .filter_map(|e| e.downcast_ref::<cob::identity::ApplyError>())
            .any(predicate)
    }

    fn execute<F, T>(&self, actor: Actor, action: F) -> Result<T, cob::identity::Error>
    where
        T: std::fmt::Debug,
        F: FnOnce(
            &mut cob::identity::IdentityMut<
                '_,
                '_,
                crate::storage::git::Repository,
                crate::node::device::Device<crypto::test::signer::MockSigner>,
            >,
        ) -> Result<T, cob::identity::Error>,
    {
        let mut identity = self.identity_for(&actor);
        cob::stable::with_advanced_timestamp(|| action(&mut identity))
    }

    fn apply(&mut self, op: &TestOp) {
        match &op.action {
            OpAction::Update { toggle_delegate } => {
                let current_id = self.identity_for(&op.actor).current;
                let is_delegate = self.is_delegate(op.actor, current_id);
                let (_, _, mut doc) = self.signer_identity_doc_for(&op.actor);

                self.toggle_delegate(toggle_delegate, &mut doc);
                self.update_description(&mut doc, "Fuzz");

                let result = self.execute(op.actor, |id| {
                    id.update(
                        cob::Title::new("Update").unwrap(),
                        "",
                        &doc.verified().unwrap(),
                    )
                });

                if is_delegate {
                    match result {
                        Ok(rev) => self.revisions.push(rev),
                        Err(e)
                            if Self::has_apply_error(&e, |err| {
                                matches!(err, cob::identity::ApplyError::DocUnchanged)
                            }) => {}
                        Err(e) => {
                            panic!("Delegate action should succeed, but failed with: {:?}", e)
                        }
                    }
                } else {
                    let e = result.expect_err("Non-delegate action must fail");
                    assert!(Self::has_apply_error(&e, |err| matches!(
                        err,
                        cob::identity::ApplyError::NonDelegateUnauthorized { .. }
                    )));
                }
            }
            OpAction::UpdateRandom {
                toggle_delegate,
                parent_idx,
            } => {
                if self.revisions.is_empty() {
                    return;
                }
                let parent_rev = self.revisions[*parent_idx % self.revisions.len()];

                if self.identity_for(&op.actor).revision(&parent_rev).is_none() {
                    return;
                }

                let is_delegate = self.is_delegate(op.actor, parent_rev);
                let (signer, _, mut doc) = self.signer_identity_doc_for(&op.actor);

                self.toggle_delegate(toggle_delegate, &mut doc);
                self.update_description(&mut doc, "Fuzz Child");

                let result = self.execute(op.actor, |id| {
                    id.transaction("Update Child", |tx, r| {
                        *tx = Transaction::new_revision(
                            cob::Title::new("Update Child").unwrap(),
                            "",
                            &doc.verified().unwrap(),
                            Some(parent_rev),
                            r,
                            signer,
                        )?;
                        Ok(())
                    })
                });

                if is_delegate {
                    match result {
                        Ok(rev) => self.revisions.push(rev),
                        Err(e)
                            if Self::has_apply_error(&e, |err| {
                                matches!(err, cob::identity::ApplyError::DocUnchanged)
                            }) => {}
                        Err(e) => {
                            panic!("Delegate action should succeed, but failed with: {:?}", e)
                        }
                    }
                } else {
                    let e = result.expect_err("Non-delegate action must fail");
                    assert!(Self::has_apply_error(&e, |err| matches!(
                        err,
                        cob::identity::ApplyError::NonDelegateUnauthorized { .. }
                    )));
                }
            }
            OpAction::Accept(idx) => {
                if self.revisions.is_empty() {
                    return;
                }
                let rev = self.revisions[*idx % self.revisions.len()];
                let identity = self.identity_for(&op.actor);
                let Some(revision) = identity.revision(&rev) else {
                    return;
                };
                let parent_id = revision.parent.unwrap();
                let is_delegate = self.is_delegate(op.actor, parent_id);
                let is_terminal = !revision.is_active();

                let result = self.execute(op.actor, |id| id.accept(&rev));

                if is_terminal {
                    assert!(result.is_ok(), "Terminal accept should be a no-op");
                } else if is_delegate {
                    match result {
                        Ok(_) => {}
                        Err(e) => assert!(
                            Self::has_apply_error(&e, |err| matches!(
                                err,
                                cob::identity::ApplyError::DuplicateVerdict
                                // An `actor` for the above `accept` could have
                                // accepted an already active, sibling revision
                                // which is disallowed.
                                    | cob::identity::ApplyError::SiblingAccepted { .. }
                            )),
                            "Delegate accept failed with unexpected error: {e:?}"
                        ),
                    }
                } else {
                    let e = result.expect_err("Non-delegate must fail on active revision");
                    assert!(
                        Self::has_apply_error(&e, |err| matches!(
                            err,
                            cob::identity::ApplyError::NonDelegateUnauthorized { .. }
                        )),
                        "Expected NonDelegateUnauthorized, got: {e:?}"
                    );
                }
            }
            OpAction::Reject(idx) => {
                if self.revisions.is_empty() {
                    return;
                }
                let rev = self.revisions[*idx % self.revisions.len()];
                let identity = self.identity_for(&op.actor);
                let Some(revision) = identity.revision(&rev) else {
                    return;
                };
                let parent_id = revision.parent.unwrap();
                let is_delegate = self.is_delegate(op.actor, parent_id);
                let is_terminal = !revision.is_active();

                let result = self.execute(op.actor, |id| id.reject(rev));

                if is_terminal {
                    assert!(result.is_ok(), "Terminal reject should be a no-op");
                } else if is_delegate {
                    match result {
                        Ok(_) => {}
                        Err(e) => assert!(
                            Self::has_apply_error(&e, |err| matches!(
                                err,
                                cob::identity::ApplyError::DuplicateVerdict
                            )),
                            "Delegate reject failed with unexpected error: {e:?}"
                        ),
                    }
                } else {
                    let e = result.expect_err("Non-delegate must fail on active revision");
                    assert!(
                        Self::has_apply_error(&e, |err| matches!(
                            err,
                            cob::identity::ApplyError::NonDelegateUnauthorized { .. }
                        )),
                        "Expected NonDelegateUnauthorized, got: {e:?}"
                    );
                }
            }
            OpAction::Redact(idx) => {
                if self.revisions.is_empty() {
                    return;
                }
                let rev = self.revisions[*idx % self.revisions.len()];
                let identity = self.identity_for(&op.actor);
                let Some(revision) = identity.revision(&rev) else {
                    return;
                };
                let is_author = revision.author.id() == &self.did_for(&op.actor);

                let result = self.execute(op.actor, |id| id.redact(rev));

                if is_author {
                    assert!(result.is_ok(), "Author should be able to redact");
                } else {
                    let e = result.expect_err("Non-author must fail to redact");
                    assert!(
                        Self::has_apply_error(&e, |err| matches!(
                            err,
                            cob::identity::ApplyError::NotAuthorized
                        )),
                        "Expected NotAuthorized, got: {e:?}"
                    );
                }
            }
            OpAction::Sync => match op.actor {
                Actor::Alice => {
                    self.network.alice.repo.fetch(&self.network.bob);
                    self.network.alice.repo.fetch(&self.network.eve);
                    self.network.alice.repo.fetch(&self.network.dave);
                }
                Actor::Bob => {
                    self.network.bob.repo.fetch(&self.network.alice);
                    self.network.bob.repo.fetch(&self.network.eve);
                    self.network.bob.repo.fetch(&self.network.dave);
                }
                Actor::Eve => {
                    self.network.eve.repo.fetch(&self.network.alice);
                    self.network.eve.repo.fetch(&self.network.bob);
                    self.network.eve.repo.fetch(&self.network.dave);
                }
                Actor::Dave => {
                    self.network.dave.repo.fetch(&self.network.alice);
                    self.network.dave.repo.fetch(&self.network.bob);
                    self.network.dave.repo.fetch(&self.network.eve);
                }
            },
        }
    }

    fn update_description(&self, doc: &mut crate::prelude::RawDoc, msg: &str) {
        let prj = doc.project().unwrap();
        let desc = format!("{} {}", msg, self.revisions.len());
        let prj = prj.update(None, desc, None).unwrap();
        doc.payload.insert(PayloadId::project(), prj.into());
    }

    fn did_for(&self, actor: &Actor) -> Did {
        match actor {
            Actor::Alice => self.network.alice.signer.public_key().into(),
            Actor::Bob => self.network.bob.signer.public_key().into(),
            Actor::Eve => self.network.eve.signer.public_key().into(),
            Actor::Dave => self.network.dave.signer.public_key().into(),
        }
    }

    fn toggle_delegate(&self, delegate: &Actor, doc: &mut crate::prelude::RawDoc) {
        let target_delegate: Did = self.did_for(delegate);

        if doc.delegates.contains(&target_delegate) {
            if doc.delegates.len() > 1 {
                doc.rescind(&target_delegate).unwrap();

                // Ensure threshold never exceeds new delegate count
                if doc.threshold > doc.delegates.len() {
                    doc.threshold = doc.delegates.len();
                }
            }
        } else {
            doc.delegate(target_delegate);
        }
    }

    fn identity_for(
        &self,
        actor: &Actor,
    ) -> cob::identity::IdentityMut<
        '_,
        '_,
        crate::storage::git::Repository,
        crate::node::device::Device<crypto::test::signer::MockSigner>,
    > {
        let (_, identity, _) = self.signer_identity_doc_for(actor);

        identity
    }

    fn signer_identity_doc_for(
        &self,
        actor: &Actor,
    ) -> (
        &crate::node::device::Device<crypto::test::signer::MockSigner>,
        cob::identity::IdentityMut<
            '_,
            '_,
            crate::storage::git::Repository,
            crate::node::device::Device<crypto::test::signer::MockSigner>,
        >,
        crate::prelude::RawDoc,
    ) {
        let (repo, signer) = match actor {
            Actor::Alice => (&*self.network.alice.repo, &self.network.alice.signer),
            Actor::Bob => (&*self.network.bob.repo, &self.network.bob.signer),
            Actor::Eve => (&*self.network.eve.repo, &self.network.eve.signer),
            Actor::Dave => (&*self.network.dave.repo, &self.network.dave.signer),
        };
        let identity = Identity::load_mut(repo, signer).unwrap();
        let doc = identity.doc().clone().edit();
        (signer, identity, doc)
    }

    fn converge(&mut self) {
        let alice = &self.network.alice;
        let bob = &self.network.bob;
        let eve = &self.network.eve;
        let dave = &self.network.dave;

        // Hub and spoke sync
        alice.repo.fetch(bob);
        alice.repo.fetch(eve);
        alice.repo.fetch(dave);

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);
        dave.repo.fetch(alice);
    }

    fn assert_local_invariants(&self) {
        let alice_diverged_id = Identity::load(&*self.network.alice.repo).unwrap();
        let bob_diverged_id = Identity::load(&*self.network.bob.repo).unwrap();
        let eve_diverged_id = Identity::load(&*self.network.eve.repo).unwrap();
        let dave_diverged_id = Identity::load(&*self.network.dave.repo).unwrap();

        self.assert_local_invariants_for(&alice_diverged_id);
        self.assert_local_invariants_for(&bob_diverged_id);
        self.assert_local_invariants_for(&eve_diverged_id);
        self.assert_local_invariants_for(&dave_diverged_id);
    }

    fn assert_local_invariants_for(&self, identity: &Identity) {
        self.assert_single_accepted_head(identity);
        self.assert_linear_history(identity);
        self.assert_active_quorum(identity);
        self.assert_accepted_quorum(identity);
        self.assert_rejected_quorum(identity);
        self.assert_sibling_rejection(identity);
        self.assert_ancestor_redacted_from_redacted_parent(identity);
        self.assert_ancestor_rejected_from_rejected_parent(identity);
        self.assert_failed_parents_have_failed_children(identity);
        self.assert_rejection_reasons_are_accurate(identity);
        self.assert_verdicts_match_parent_delegates(identity);
    }

    /// Strong eventual consistency.
    ///
    /// Nodes can receive operations in different orders. Regardless of the path taken,
    /// once nodes sync the same data, their state machines must compute the exact same
    /// state.
    ///
    /// PASS:
    ///   Alice (Ops: 1, 2, 3) => State A
    ///   Bob   (Ops: 3, 1, 2) => State A
    ///
    /// FAIL:
    ///   Alice (Ops: 1, 2, 3) => State A
    ///   Bob   (Ops: 2, 1, 3) => State B (non-commutative)
    fn assert_convergence(&self) {
        let alice = Identity::load(&*self.network.alice.repo).unwrap();
        let bob = Identity::load(&*self.network.bob.repo).unwrap();
        let eve = Identity::load(&*self.network.eve.repo).unwrap();
        let dave = Identity::load(&*self.network.dave.repo).unwrap();

        assert_eq!(alice.current, bob.current, "Alice and Bob must converge");
        assert_eq!(bob.current, eve.current, "Bob and Eve must converge");
        assert_eq!(eve.current, dave.current, "Eve and Dave must converge");
        assert_eq!(dave.current, alice.current, "Dave and Alice must converge");
    }

    /// The `current` pointer always points to a valid, `Accepted` revision.
    ///
    /// The repository must have a single, unambiguous canonical identity document.
    /// The head of the identity cannot be a pending, rejected, or redacted proposal.
    ///
    /// PASS:
    ///   current == [Accepted]
    ///
    /// FAIL:
    ///   current == [Active]   (Pointer moved before quorum was reached)
    ///   current == [Redacted] (The canonical head was illegally redacted)
    fn assert_single_accepted_head(&self, identity: &Identity) {
        let accepted_count = identity
            .revisions()
            .filter(|r| r.state == State::Accepted && r.id == identity.current)
            .count();
        assert_eq!(
            accepted_count, 1,
            "Exactly one accepted revision must match current"
        );
    }

    /// The causal chain is unbroken and valid.
    ///
    /// 1. We cannot have "zombie" children (Active revisions whose parents are redacted/rejected).
    /// 2. The canonical history must be a straight line of accepted states back to the root.
    ///
    /// PASS:
    ///   [Accepted] <-- [Accepted] <-- [Active] (Valid pending proposal)
    ///
    /// FAIL:
    ///   [Rejected] <-- [Active] (Zombie child, parent is dead but child is pending)
    ///   [Active]   <-- [Accepted] (Broken chain, an accepted state has an unaccepted parent)
    fn assert_linear_history(&self, identity: &Identity) {
        // No active revision has a rejected/redacted parent
        for rev in identity.revisions() {
            if rev.state == State::Active
                && let Some(parent_id) = rev.parent
                && let Some(parent) = identity.revision(&parent_id)
            {
                assert!(
                    !matches!(parent.state, State::Rejected(_) | State::Redacted(_)),
                    "Active revision {} has invalid parent state {:?}",
                    rev.id,
                    parent.state
                );
            }
        }

        // All ancestors of the current revision must be Accepted.
        let mut curr_id = identity.current;
        while let Some(rev) = identity.revision(&curr_id) {
            assert_eq!(
                rev.state,
                State::Accepted,
                "Parent {} in the canonical chain must be Accepted",
                rev.id
            );
            if let Some(parent_id) = rev.parent {
                curr_id = parent_id;
            } else {
                break;
            }
        }
    }

    /// No `Active` revision has met the required majority.
    ///
    /// If an active revision gathers enough votes, the state machine should
    /// have automatically transitioned it to `Accepted`. If it is still `Active`,
    /// the `adopt()` trigger failed to fire.
    ///
    /// PASS:
    ///   [Active, 1/2 votes]
    ///
    /// FAIL:
    ///   [Active, 2/2 votes] (Should have been Accepted)
    fn assert_active_quorum(&self, identity: &Identity) {
        for rev in identity.revisions() {
            if rev.state == State::Active {
                let parent_id = rev.parent.expect("Active revision must have a parent");

                // If the parent is not the current document, it is perfectly valid for this
                // revision to have reached a majority while still being `Active`.
                // It is queued and waiting for its parent to be adopted first.
                if parent_id != identity.current {
                    continue;
                }

                let parent = identity.revision(&parent_id).unwrap();
                let majority = parent.doc.majority();

                assert!(
                    rev.accepted().count() < majority,
                    "Active revision {} has {} votes but majority is {}. It should have been adopted!",
                    rev.id,
                    rev.accepted().count(),
                    majority
                );
            }
        }
    }

    /// Every `Accepted` revision actually met the required majority.
    ///
    /// Prevents illegally accepted revisions. A revision cannot bypass the voting
    /// rules to become accepted.
    ///
    /// PASS:
    ///   [Accepted, 2/2 votes]
    ///
    /// FAIL:
    ///   [Accepted, 1/2 votes] (Accepted without reaching quorum)
    fn assert_accepted_quorum(&self, identity: &Identity) {
        for rev in identity.revisions() {
            if rev.state == State::Accepted && rev.id != identity.root {
                let parent_id = rev.parent.expect("Accepted revision must have a parent");
                let parent = identity.revision(&parent_id).unwrap();
                let majority = parent.doc.majority();

                assert!(
                    rev.accepted().count() >= majority,
                    "Accepted revision {} only has {} votes, but needed {}!",
                    rev.id,
                    rev.accepted().count(),
                    majority
                );
            }
        }
    }

    /// Every `Rejected(Vote)` revision actually met the required majority.
    ///
    /// Prevents illegally rejected revisions. A revision cannot bypass the voting
    /// rules to become rejected.
    ///
    /// PASS (3 delegates, majority 2):
    ///   [Rejected(Vote), 2/3 reject votes] (Only 1 delegate left, impossible to reach 2 accepts)
    ///
    /// FAIL (3 delegates, majority 2):
    ///   [Rejected(Vote), 1/3 reject votes] (2 delegates left, still possible to reach 2 accepts)
    fn assert_rejected_quorum(&self, identity: &Identity) {
        for rev in identity.revisions() {
            if rev.state == State::Rejected(RejectedBy::Vote) && rev.id != identity.root {
                let parent_id = rev.parent.expect("Rejected revision must have a parent");
                let parent = identity.revision(&parent_id).unwrap();

                let rejection_threshold = parent.doc.delegates().len() - parent.doc.majority();
                let needed_majority = rejection_threshold + 1;

                assert!(
                    rev.rejected().count() >= needed_majority,
                    "RejectedBy::Vote revision {} only has {} votes, but needed {}!",
                    rev.id,
                    rev.rejected().count(),
                    needed_majority
                );
            }
        }
    }

    /// There can be only one accepted sibling.
    ///
    /// When a revision is accepted, all competing forks must be explicitly
    /// rejected to prevent split-brain scenarios and enforce a linear history.
    ///
    /// PASS:
    ///             /-- [Accepted]
    ///   [Parent]<
    ///             \-- [Rejected]
    ///
    /// FAIL:
    ///             /-- [Accepted]
    ///   [Parent]<
    ///             \-- [Active]   (Sibling was not rejected)
    fn assert_sibling_rejection(&self, identity: &Identity) {
        for rev in identity.revisions() {
            if rev.state == State::Accepted {
                for sibling in identity.revisions() {
                    // If they share the same parent but have different IDs, they are siblings
                    if sibling.id != rev.id && sibling.parent == rev.parent {
                        assert!(
                            matches!(sibling.state, State::Rejected(_) | State::Redacted(_)),
                            "Sibling {} of accepted revision {} must be rejected or redacted, but is {:?}",
                            sibling.id,
                            rev.id,
                            sibling.state
                        );
                    }
                }
            }
        }
    }

    /// A revision can only be `Rejected(RejedtedBy::Parent)` if its parent was rejected.
    ///
    /// Ensures that the `RejectedBy::Parent` state is causally linked to a parent's rejection.
    ///
    /// PASS:
    ///   [Rejected] <-- [Rejected(RejectedBy::Parent)]
    ///
    /// FAIL:
    ///   [Active]   <-- [Rejected(RejectedBy::Parent)] (Parent is not rejected)
    fn assert_ancestor_rejected_from_rejected_parent(&self, identity: &Identity) {
        for id in &identity.timeline {
            if let Some(rev) = identity.revision(id)
                && matches!(rev.state, State::Rejected(RejectedBy::Parent))
            {
                let parent_id = rev
                    .parent
                    .expect("a revision in `RejectedBy::Parent` must have a parent");
                let parent = identity.revision(&parent_id).unwrap();
                assert!(
                    matches!(
                        parent.state,
                        State::Rejected(RejectedBy::Vote)
                            | State::Rejected(RejectedBy::Sibling(_))
                            | State::Rejected(RejectedBy::Parent)
                    ),
                    "RejectedBy::Parent revision {} has invalid parent state {:?}",
                    rev.id,
                    parent.state
                );
            }
        }
    }

    /// A revision can only be `Redacted(RedactedBy::Parent)` if its parent was redacted.
    ///
    /// Ensures that the `RedactedBy::Parent` state is causally linked to a parent's redaction.
    ///
    /// PASS:
    ///   [Redacted] <-- [Redacted(RedactedBy::Parent)]
    ///
    /// FAIL:
    ///   [Accepted] <-- [Redacted(RedactedBy::Parent)] (Parent is not redacted)
    fn assert_ancestor_redacted_from_redacted_parent(&self, identity: &Identity) {
        for id in &identity.timeline {
            if let Some(rev) = identity.revision(id)
                && matches!(rev.state, State::Redacted(RedactedBy::Parent))
            {
                let parent_id = rev
                    .parent
                    .expect("a revision in `RedactedBy::Parent` must have a parent");
                let parent = identity.revision(&parent_id).unwrap();
                assert!(
                    matches!(
                        parent.state,
                        State::Redacted(RedactedBy::Author) | State::Redacted(RedactedBy::Parent)
                    ),
                    "RedactedBy::Parent revision {} has invalid parent state {:?}",
                    rev.id,
                    parent.state
                );
            }
        }
    }

    /// If a parent is in a failed state, it cannot have any `Active` or `Accepted` children.
    ///
    /// Enforces the "cascade" rule. When a branch dies (via rejection or redaction),
    /// all of its pending children must also die. A dead branch cannot produce canonical history.
    /// Note that a child can still be explicitly `Rejected(Vote)` or `Redacted(Author)` on its
    /// own merits.
    ///
    /// PASS:
    ///   [Rejected] <-- [Rejected(RejectedBy::Parent)]
    ///   [Rejected] <-- [Rejected(Vote)] (Child was explicitly voted down before parent died)
    ///
    /// FAIL:
    ///   [Rejected] <-- [Active] (Zombie child)
    fn assert_failed_parents_have_failed_children(&self, identity: &Identity) {
        for id in &identity.timeline {
            if let Some(rev) = identity.revision(id)
                && let Some(parent_id) = rev.parent
                && let Some(parent) = identity.revision(&parent_id)
            {
                let parent_is_failed = matches!(
                    parent.state,
                    State::Rejected(RejectedBy::Vote)
                        | State::Rejected(RejectedBy::Parent)
                        | State::Rejected(RejectedBy::Sibling(_))
                        | State::Redacted(RedactedBy::Author)
                        | State::Redacted(RedactedBy::Parent)
                );

                if parent_is_failed {
                    assert!(
                        !matches!(rev.state, State::Active | State::Accepted),
                        "Child {} is {:?} but parent {} has failed ({:?})",
                        rev.id,
                        rev.state,
                        parent.id,
                        parent.state
                    );
                }
            }
        }
    }

    /// Validates that the [`Oid`]s inside terminal states point to the correct revisions.
    ///
    /// Ensures that `RejectedBy::Sibling` points to a true sibling that actually won (`Accepted`),
    /// and that `RejectedBy::Parent` variants point to a true parent in a failed state. Prevents broken
    /// causal links like pointing to an "uncle" or a revision on a completely different branch.
    ///
    /// PASS:
    ///   [Accepted (A)] and [RejectedBy::Sibling(A)] share the same parent.
    ///
    /// FAIL:
    ///   [RejectedBy::Sibling(B)] where B does not share the same parent.
    ///   [RejectedBy::Parent] without a rejedted parent.
    fn assert_rejection_reasons_are_accurate(&self, identity: &Identity) {
        for id in &identity.timeline {
            let Some(rev) = identity.revision(id) else {
                continue;
            };

            match rev.state {
                State::Rejected(RejectedBy::Sibling(sibling_id)) => {
                    let sibling = identity.revision(&sibling_id).expect("Sibling must exist");

                    assert_eq!(
                        sibling.state,
                        State::Accepted,
                        "The winning sibling {} must be Accepted",
                        sibling_id
                    );
                    assert_eq!(
                        sibling.parent, rev.parent,
                        "Revision {} and {} must share the same parent",
                        rev.id, sibling_id
                    );
                    assert_ne!(
                        sibling.id, rev.id,
                        "A revision cannot be rejected by itself"
                    );
                }
                State::Rejected(RejectedBy::Parent) => {
                    let parent_id = rev.parent.expect("revision is not the root revision");
                    let parent = identity.revision(&parent_id).expect("parent exists");

                    assert_matches!(
                        parent.state,
                        State::Rejected(_),
                        "Parent {parent_id} must be rejected state",
                    );
                }
                State::Redacted(RedactedBy::Parent) => {
                    let parent_id = rev.parent.expect("revision is not the root revision");
                    let parent = identity.revision(&parent_id).expect("parent exists");

                    assert_matches!(
                        parent.state,
                        State::Redacted(_),
                        "Parent {parent_id} must be redacted",
                    );
                }
                _ => {}
            }
        }
    }

    /// Every vote on a revision must be cast by an authorized delegate according to its causal parent.
    ///
    /// Ensures that authorization is strictly governed by the causal history of the document,
    /// rather than the network's currently accepted state. A user cannot cast a verdict on a
    /// revision if they are not a delegate in that specific revision's parent.
    ///
    /// PASS:
    ///   [Parent: Alice, Bob are delegates] <-- [Child: Bob votes Accept]
    ///
    /// FAIL:
    ///   [Parent: Alice is sole delegate] <-- [Child: Bob votes Accept] (Unauthorized)
    ///   [Parent: Removes Bob] <-- [Child: Bob votes Accept] (Bob is no longer a delegate)
    fn assert_verdicts_match_parent_delegates(&self, identity: &Identity) {
        for id in &identity.timeline {
            let Some(rev) = identity.revision(id) else {
                continue;
            };
            if let Some(parent_id) = rev.parent {
                let parent = identity.revision(&parent_id).unwrap();
                for (voter_key, _) in rev.verdicts() {
                    let voter_did = Did::from(*voter_key);
                    assert!(
                        parent.doc.is_delegate(&voter_did),
                        "Voter {} cast a verdict on revision {} but is not a delegate in parent {}",
                        voter_did,
                        rev.id,
                        parent_id
                    );
                }
            }
        }
    }
}

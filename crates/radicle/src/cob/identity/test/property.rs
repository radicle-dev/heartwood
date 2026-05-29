use qcheck::{Arbitrary, TestResult};
use qcheck_macros::quickcheck;

use crate::cob;
use crate::cob::identity::{Did, Identity, RevisionId, State};
use crate::cob::store::Transaction;
use crate::identity::doc::PayloadId;
use crate::test::setup::Network;

#[derive(Clone, Debug)]
enum Actor {
    Alice,
    Bob,
    Eve,
}

#[derive(Clone, Debug)]
enum OpAction {
    Update {
        toggle_delegate: u8,
    },
    UpdateRandom {
        toggle_delegate: u8,
        parent_idx: usize,
    },
    Accept(usize),
    Reject(usize),
    Redact(usize),
    Sync,
}

#[derive(Clone, Debug)]
struct TestOp {
    actor: Actor,
    action: OpAction,
}

impl Arbitrary for TestOp {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let actor = match u8::arbitrary(g) % 3 {
            0 => Actor::Alice,
            1 => Actor::Bob,
            _ => Actor::Eve,
        };

        let action = match u8::arbitrary(g) % 6 {
            0 => OpAction::Update {
                toggle_delegate: u8::arbitrary(g) % 3,
            },
            1 => OpAction::UpdateRandom {
                toggle_delegate: u8::arbitrary(g) % 3,
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

    fn apply(&mut self, op: &TestOp) {
        match &op.action {
            OpAction::Update { toggle_delegate } => {
                let (repo, signer) = match op.actor {
                    Actor::Alice => (&*self.network.alice.repo, &self.network.alice.signer),
                    Actor::Bob => (&*self.network.bob.repo, &self.network.bob.signer),
                    Actor::Eve => (&*self.network.eve.repo, &self.network.eve.signer),
                };
                let mut identity = Identity::load_mut(repo, signer).unwrap();
                let mut doc = identity.doc().clone().edit();

                let target_delegate: Did = match toggle_delegate {
                    0 => self.network.alice.signer.public_key().into(),
                    1 => self.network.bob.signer.public_key().into(),
                    _ => self.network.eve.signer.public_key().into(),
                };

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

                let prj = doc.project().unwrap();
                let desc = format!("Fuzz {}", self.revisions.len());
                let prj = prj.update(None, desc, None).unwrap();
                doc.payload.insert(PayloadId::project(), prj.into());

                if let Ok(rev) = cob::stable::with_advanced_timestamp(|| {
                    identity.update(
                        cob::Title::new("Update").unwrap(),
                        "",
                        &doc.verified().unwrap(),
                    )
                }) {
                    self.revisions.push(rev);
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

                let (repo, signer) = match op.actor {
                    Actor::Alice => (&*self.network.alice.repo, &self.network.alice.signer),
                    Actor::Bob => (&*self.network.bob.repo, &self.network.bob.signer),
                    Actor::Eve => (&*self.network.eve.repo, &self.network.eve.signer),
                };
                let mut identity = Identity::load_mut(repo, signer).unwrap();
                let mut doc = identity.doc().clone().edit();

                let target_delegate: Did = match toggle_delegate {
                    0 => self.network.alice.signer.public_key().into(),
                    1 => self.network.bob.signer.public_key().into(),
                    _ => self.network.eve.signer.public_key().into(),
                };

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

                let prj = doc.project().unwrap();
                let desc = format!("Fuzz Child {}", self.revisions.len());
                let prj = prj.update(None, desc, None).unwrap();
                doc.payload.insert(PayloadId::project(), prj.into());

                // Use a manual transaction to force the parent to be `parent_rev`
                if let Ok(rev) = cob::stable::with_advanced_timestamp(|| {
                    identity.transaction("Update Child", |tx, r| {
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
                }) {
                    self.revisions.push(rev);
                }
            }
            OpAction::Accept(idx) => {
                if self.revisions.is_empty() {
                    return;
                }
                let rev = self.revisions[*idx % self.revisions.len()];
                let (repo, signer) = match op.actor {
                    Actor::Alice => (&*self.network.alice.repo, &self.network.alice.signer),
                    Actor::Bob => (&*self.network.bob.repo, &self.network.bob.signer),
                    Actor::Eve => (&*self.network.eve.repo, &self.network.eve.signer),
                };
                let mut identity = Identity::load_mut(repo, signer).unwrap();
                let _ = cob::stable::with_advanced_timestamp(|| identity.accept(&rev));
            }
            OpAction::Reject(idx) => {
                if self.revisions.is_empty() {
                    return;
                }
                let rev = self.revisions[*idx % self.revisions.len()];
                let (repo, signer) = match op.actor {
                    Actor::Alice => (&*self.network.alice.repo, &self.network.alice.signer),
                    Actor::Bob => (&*self.network.bob.repo, &self.network.bob.signer),
                    Actor::Eve => (&*self.network.eve.repo, &self.network.eve.signer),
                };
                let mut identity = Identity::load_mut(repo, signer).unwrap();
                let _ = cob::stable::with_advanced_timestamp(|| identity.reject(rev));
            }
            OpAction::Redact(idx) => {
                if self.revisions.is_empty() {
                    return;
                }
                let rev = self.revisions[*idx % self.revisions.len()];
                let (repo, signer) = match op.actor {
                    Actor::Alice => (&*self.network.alice.repo, &self.network.alice.signer),
                    Actor::Bob => (&*self.network.bob.repo, &self.network.bob.signer),
                    Actor::Eve => (&*self.network.eve.repo, &self.network.eve.signer),
                };
                let mut identity = Identity::load_mut(repo, signer).unwrap();
                let _ = cob::stable::with_advanced_timestamp(|| identity.redact(rev));
            }
            OpAction::Sync => match op.actor {
                Actor::Alice => {
                    self.network.alice.repo.fetch(&self.network.bob);
                    self.network.alice.repo.fetch(&self.network.eve);
                }
                Actor::Bob => {
                    self.network.bob.repo.fetch(&self.network.alice);
                    self.network.bob.repo.fetch(&self.network.eve);
                }
                Actor::Eve => {
                    self.network.eve.repo.fetch(&self.network.alice);
                    self.network.eve.repo.fetch(&self.network.bob);
                }
            },
        }
    }

    fn converge(&mut self) {
        let alice = &self.network.alice;
        let bob = &self.network.bob;
        let eve = &self.network.eve;

        // Hub and spoke sync
        alice.repo.fetch(bob);
        alice.repo.fetch(eve);

        bob.repo.fetch(alice);
        eve.repo.fetch(alice);
    }

    fn assert_local_invariants(&self, identity: &Identity) {
        self.assert_single_accepted_head(identity);
        self.assert_linear_history(identity);
        self.assert_active_quorum(identity);
        self.assert_accepted_quorum(identity);
        self.assert_sibling_rejection(identity);
        self.assert_ancestor_redacted_from_redacted_parent(identity);
        self.assert_ancestor_rejected_from_rejected_parent(identity);
        self.assert_failed_parents_have_failed_children(identity);
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
    fn assert_convergence(&self, alice: &Identity, bob: &Identity, eve: &Identity) {
        assert_eq!(alice.current, bob.current, "Alice and Bob must converge");
        assert_eq!(bob.current, eve.current, "Bob and Eve must converge");
        assert_eq!(eve.current, alice.current, "Eve and Alice must converge");
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
                    parent.state != State::Rejected && parent.state != State::Redacted,
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
                "Ancestor {} in the canonical chain must be Accepted",
                rev.id
            );
            if let Some(parent_id) = rev.parent {
                curr_id = parent_id;
            } else {
                break; // Reached the root
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

                // If the parent is not the current document, it is valuleded for this revision
                // to have a majority. It is queued for later.
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
                            sibling.state == State::Rejected || sibling.state == State::Redacted,
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

    /// A revision can only be `AncestorRejected` if its parent was rejected.
    fn assert_ancestor_rejected_from_rejected_parent(&self, identity: &Identity) {
        for id in &identity.timeline {
            if let Some(rev) = identity.revision(id)
                && rev.state == State::AncestorRejected
            {
                let parent_id = rev.parent.expect("AncestorRejected must have a parent");
                let parent = identity.revision(&parent_id).unwrap();
                assert!(
                    parent.state == State::Rejected || parent.state == State::AncestorRejected,
                    "AncestorRejected revision {} has invalid parent state {:?}",
                    rev.id,
                    parent.state
                );
            }
        }
    }

    /// A revision can only be `AncestorRedacted` if its parent was redacted.
    fn assert_ancestor_redacted_from_redacted_parent(&self, identity: &Identity) {
        for id in &identity.timeline {
            if let Some(rev) = identity.revision(id)
                && rev.state == State::AncestorRedacted
            {
                let parent_id = rev.parent.expect("AncestorRedacted must have a parent");
                let parent = identity.revision(&parent_id).unwrap();
                assert!(
                    parent.state == State::Redacted || parent.state == State::AncestorRedacted,
                    "AncestorRedacted revision {} has invalid parent state {:?}",
                    rev.id,
                    parent.state
                );
            }
        }
    }

    /// If a parent is in a failed state, it cannot have any `Active` or `Accepted` children.
    fn assert_failed_parents_have_failed_children(&self, identity: &Identity) {
        for id in &identity.timeline {
            if let Some(rev) = identity.revision(id)
                && let Some(parent_id) = rev.parent
                && let Some(parent) = identity.revision(&parent_id)
            {
                let parent_is_failed = matches!(
                    parent.state,
                    State::Rejected
                        | State::AncestorRejected
                        | State::Redacted
                        | State::AncestorRedacted
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
}

#[quickcheck]
fn prop_invariants(ops: Vec<TestOp>) -> TestResult {
    let ops = if ops.len() > 15 {
        ops[..15].to_vec()
    } else {
        ops
    };
    if ops.is_empty() {
        return TestResult::discard();
    }

    let mut harness = Harness::new();

    for op in ops {
        harness.apply(&op);
    }

    let alice_diverged_id = Identity::load(&*harness.network.alice.repo).unwrap();
    let bob_diverged_id = Identity::load(&*harness.network.bob.repo).unwrap();
    let eve_diverged_id = Identity::load(&*harness.network.eve.repo).unwrap();

    harness.assert_local_invariants(&alice_diverged_id);
    harness.assert_local_invariants(&bob_diverged_id);
    harness.assert_local_invariants(&eve_diverged_id);

    harness.converge();

    let alice_converged_id = Identity::load(&*harness.network.alice.repo).unwrap();
    let bob_converged_id = Identity::load(&*harness.network.bob.repo).unwrap();
    let eve_converged_id = Identity::load(&*harness.network.eve.repo).unwrap();

    harness.assert_local_invariants(&alice_converged_id);
    harness.assert_local_invariants(&bob_converged_id);
    harness.assert_local_invariants(&eve_converged_id);

    harness.assert_convergence(&alice_converged_id, &bob_converged_id, &eve_converged_id);

    TestResult::passed()
}

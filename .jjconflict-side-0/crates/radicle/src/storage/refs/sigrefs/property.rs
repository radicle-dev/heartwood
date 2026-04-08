#![allow(clippy::unwrap_used)]

mod mock;
use mock::*;

use crypto::Signer as _;
use crypto::test::signer::MockSigner;
use qcheck::TestResult;
use qcheck_macros::quickcheck;

use crate::storage::refs::Refs;
use crate::storage::refs::sigrefs::read::{SignedRefsReader, Tip};
use crate::storage::refs::sigrefs::write::{SignedRefsWriter, Update};

#[quickcheck]
fn roundtrip(BoundedVec(all_refs): BoundedVec<Refs>) -> TestResult {
    if all_refs.is_empty() {
        return TestResult::discard();
    }

    let fixture = Fixture::new();
    let signer = MockSigner::default();
    let node_id = *signer.public_key();

    for refs in all_refs {
        let refs = fixture.with_identity_root(refs);

        let writer = SignedRefsWriter::new(
            refs.clone(),
            fixture.rid(),
            node_id,
            fixture.repo(),
            &signer,
        );
        let update = match writer.write(
            fixture.committer(),
            "roundtrip write".into(),
            "roundtrip reflog".into(),
        ) {
            Ok(u) => u,
            Err(e) => return TestResult::error(format!("write error: {e}")),
        };

        let written_refs = match update {
            Update::Changed { ref entry, .. } => entry.clone().into_refs(),
            Update::Unchanged { ref verified, .. } => verified.commit().refs().clone(),
        };

        assert_eq!(refs, written_refs);

        let reader = SignedRefsReader::new(
            fixture.rid(),
            Tip::Reference(node_id),
            fixture.repo(),
            &node_id,
        );
        let verified = match reader.read() {
            Ok(v) => v,
            Err(e) => return TestResult::error(format!("read error: {e}")),
        };

        if written_refs != *verified.commit().refs() {
            return TestResult::failed();
        }
    }

    TestResult::passed()
}

#[quickcheck]
fn idempotent(refs: Refs) -> TestResult {
    let fixture = Fixture::new();
    let refs = fixture.with_identity_root(refs);
    let signer = MockSigner::default();
    let node_id = *signer.public_key();

    if let Err(e) = SignedRefsWriter::new(
        refs.clone(),
        fixture.rid(),
        node_id,
        fixture.repo(),
        &signer,
    )
    .write(
        fixture.committer(),
        "first write".into(),
        "first reflog".into(),
    ) {
        return TestResult::error(format!("first write error: {e}"));
    }

    match SignedRefsWriter::new(
        refs.clone(),
        fixture.rid(),
        node_id,
        fixture.repo(),
        &signer,
    )
    .write(
        fixture.committer(),
        "second write".into(),
        "second reflog".into(),
    ) {
        Ok(Update::Unchanged { .. }) => TestResult::passed(),
        Ok(Update::Changed { .. }) => {
            TestResult::error("expected Update::Unchanged on second write with identical refs")
        }
        Err(e) => TestResult::error(format!("second write error: {e}")),
    }
}

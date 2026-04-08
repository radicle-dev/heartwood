use super::mock;
use super::mock::{AlwaysVerify, MockRepository};
use crate::storage::refs::IDENTITY_ROOT;
use crate::storage::refs::sigrefs::SignedRefsReader;
use crate::storage::refs::sigrefs::read::Tip;
use crate::storage::refs::sigrefs::read::error;

#[test]
fn missing_sigrefs() {
    let namespace = mock::node_id();
    let repo = MockRepository::new().with_missing_rad_sigrefs(&namespace);

    let result = SignedRefsReader::new(
        mock::rid(99),
        Tip::Reference(namespace),
        &repo,
        &AlwaysVerify,
    )
    .read();

    assert!(matches!(result, Err(error::Read::MissingSigrefs { .. })));
}

#[test]
fn find_reference_error() {
    let namespace = mock::node_id();
    let repo = MockRepository::new().with_rad_sigrefs_error(&namespace);

    let result = SignedRefsReader::new(
        mock::rid(99),
        Tip::Reference(namespace),
        &repo,
        &AlwaysVerify,
    )
    .read();

    assert!(matches!(result, Err(error::Read::FindReference(_))));
}

#[test]
fn resolve_tip_ok() {
    let namespace = mock::node_id();
    let root = mock::oid(1);
    let refs = [
        (mock::refs_heads_main(), mock::oid(10)),
        (
            IDENTITY_ROOT.to_ref_string(),
            mock::oid(mock::MOCKED_IDENTITY),
        ),
    ];

    let repo = MockRepository::new()
        .with_rad_sigrefs(&namespace, root)
        .with_commit(root, mock::commit_data([]))
        .with_refs(root, refs)
        .with_signature(root, 1);

    let vc = SignedRefsReader::new(
        mock::rid(99),
        Tip::Reference(namespace),
        &repo,
        &AlwaysVerify,
    )
    .read()
    .unwrap();
    assert_eq!(vc.commit.oid, root);
}

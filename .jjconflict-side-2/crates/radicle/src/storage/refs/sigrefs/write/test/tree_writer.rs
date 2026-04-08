use super::mock;
use super::mock::MockRepository;
use crate::storage::refs::Refs;
use crate::storage::refs::sigrefs::write::{Tree, TreeWriter, error};

fn mock_refs() -> Refs {
    Refs::from([(mock::refs_heads_main(), mock::oid(10))].into_iter())
}

#[test]
fn sign_error() {
    // `NeverSign` fails before `write_tree` is ever called, so the
    // repository needs no configuration.
    let result = TreeWriter::new(mock_refs(), &MockRepository::new(), &mock::NeverSign).write();
    assert!(matches!(result, Err(error::Tree::Sign(_))));
}

#[test]
fn write_tree_error() {
    let repo = MockRepository::new().with_write_tree_error();
    let result = TreeWriter::new(mock_refs(), &repo, &mock::AlwaysSign).write();
    assert!(matches!(result, Err(error::Tree::Write(_))));
}

#[test]
fn write_ok() {
    let refs = mock_refs();
    let expected_oid = mock::oid(1);
    let repo = MockRepository::new().with_write_tree_ok(expected_oid);
    let tree = TreeWriter::new(refs.clone(), &repo, &mock::AlwaysSign)
        .write()
        .unwrap();
    assert_eq!(
        tree,
        Tree {
            oid: expected_oid,
            refs,
            signature: mock::AlwaysSign::signature(),
        }
    );
}

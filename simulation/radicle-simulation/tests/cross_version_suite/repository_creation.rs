use crate::network;
use crate::require_network;
use radicle_simulation::node::Node;
use rstest::rstest;

// TODO(Ade): The `negX` numbering is horrible... need a better naming convention.
#[rstest]
#[case::v_neg4_single_commit(network::peer_relative(-4, 0), 1)]
#[case::v_neg4_multiple_commits(network::peer_relative(-4, 0), 3)]
#[case::v_neg3_single_commit(network::peer_relative(-3, 0), 1)]
#[case::v_neg3_multiple_commits(network::peer_relative(-3, 0), 3)]
#[case::v_neg2_single_commit(network::peer_relative(-2, 0), 1)]
#[case::v_neg2_multiple_commits(network::peer_relative(-2, 0), 3)]
#[case::v_neg1_single_commit(network::peer_relative(-1, 0), 1)]
#[case::v_neg1_multiple_commits(network::peer_relative(-1, 0), 3)]
#[case::v_current_single_commit(network::peer_relative(0, 0), 1)]
#[case::v_current_multiple_commits(network::peer_relative(0, 0), 3)]
fn initializes_valid_repositories_with_single_and_multiple_commits(
    #[case] mut node: Node,
    #[case] commits: u32,
) -> Result<(), String> {
    let _guard = require_network();

    node = node.as_alice()?;

    let repo = node.init_test_repo("Repo", "Test Repo", commits)?;

    assert!(
        repo.rid.starts_with("rad:"),
        "Expected valid RID, got: {}",
        repo.rid
    );

    Ok(())
}

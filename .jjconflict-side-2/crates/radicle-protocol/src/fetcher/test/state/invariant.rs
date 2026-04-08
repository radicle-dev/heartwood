use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::command;
use crate::fetcher::test::state::helpers;
use crate::fetcher::{FetchConfig, FetcherState};

#[test]
fn queue_integrity_after_merge() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let repo_2: RepoId = arbitrary::r#gen(1);
    let refs_2a = helpers::gen_refs(1);
    let refs_2b = helpers::gen_refs(1);
    let config = FetchConfig::default();

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: helpers::gen_refs(1),
        config,
    });

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs: refs_2a.clone(),
        config,
    });

    // Second fetch for same repo - should merge
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs: refs_2b.clone(),
        config,
    });

    // Queue should have exactly one repo_2 entry (merged)
    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    let first = state.dequeue(&node_a);
    assert!(first.is_some());
    assert_eq!(first.unwrap().rid, repo_2);

    let second = state.dequeue(&node_a);
    assert!(second.is_none());
}

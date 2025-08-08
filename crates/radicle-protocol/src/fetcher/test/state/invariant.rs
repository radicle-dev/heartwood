use std::time::Duration;

use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::command;
use crate::fetcher::test::state::helpers;
use crate::fetcher::FetcherState;

#[test]
fn queue_integrity_after_merge() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let refs_at_2a = helpers::gen_refs_at(1);
    let refs_at_2b = helpers::gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: refs_at_2a.clone(),
        timeout,
    });

    // Second fetch for same repo - should merge
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: refs_at_2b.clone(),
        timeout,
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

use std::time::Duration;

use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::FetcherState;

#[test]
fn interleaved_operations() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let node_b: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    // fetch(A, r1)
    let e1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    assert!(matches!(e1, event::Fetch::Started { .. }));

    // fetch(B, r2)
    let e2 = state.fetch(command::Fetch {
        from: node_b,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    assert!(matches!(e2, event::Fetch::Started { .. }));

    // fetched(A, r1)
    let e3 = state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    assert!(matches!(e3, event::Fetched::Completed { .. }));

    // fetch(A, r3)
    let e4 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    assert!(matches!(e4, event::Fetch::Started { .. }));

    // fetched(B, r2)
    let e5 = state.fetched(command::Fetched {
        from: node_b,
        rid: repo_2,
    });
    assert!(matches!(e5, event::Fetched::Completed { .. }));

    // Final state: only r3 from A ongoing
    assert!(state.get_active_fetch(&repo_1).is_none());
    assert!(state.get_active_fetch(&repo_2).is_none());
    assert!(state.get_active_fetch(&repo_3).is_some());
}

#[test]
fn fetched_then_cancel() {
    let mut state = FetcherState::new(helpers::config(2, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
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
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    // Complete repo_1
    let e1 = state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    assert!(matches!(e1, event::Fetched::Completed { .. }));

    // Cancel remaining
    let e2 = state.cancel(command::Cancel { from: node_a });
    match e2 {
        event::Cancel::Canceled {
            active: ongoing, ..
        } => {
            assert_eq!(ongoing.len(), 1);
            assert!(ongoing.contains_key(&repo_2));
        }
        _ => panic!("Expected Canceled"),
    }
}

use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::{FetchConfig, FetcherState};

#[test]
fn complete_single_ongoing() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let refs_1 = helpers::gen_refs(2);
    let config = FetchConfig::default();

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: refs_1.clone(),
        config,
    });

    let event = state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });

    assert_eq!(
        event,
        event::Fetched::Completed {
            from: node_a,
            rid: repo_1,
            refs: refs_1,
        }
    );
    assert!(state.get_active_fetch(&repo_1).is_none());
}

#[test]
fn complete_then_dequeue_fifo() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let repo_2: RepoId = arbitrary::r#gen(1);
    let repo_3: RepoId = arbitrary::r#gen(1);
    let refs_2 = helpers::gen_refs(1);
    let config = FetchConfig::default();

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: helpers::gen_refs(1),
        config,
    });

    // Queue repo_2 first, then repo_3
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs: refs_2.clone(),
        config,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs: helpers::gen_refs(1),
        config,
    });

    let event = state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });

    assert!(matches!(event, event::Fetched::Completed { .. }));

    // Dequeue next - FIFO: repo_2 was queued first
    let queued = state.dequeue(&node_a);
    assert!(queued.is_some());
    let queued = queued.unwrap();
    assert_eq!(queued.rid, repo_2);
    assert_eq!(queued.refs, refs_2);
}

#[test]
fn complete_one_of_multiple() {
    let mut state = FetcherState::new(helpers::config(3, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let repo_2: RepoId = arbitrary::r#gen(1);
    let repo_3: RepoId = arbitrary::r#gen(1);
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
        refs: helpers::gen_refs(1),
        config,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs: helpers::gen_refs(1),
        config,
    });

    let event = state.fetched(command::Fetched {
        from: node_a,
        rid: repo_2,
    });

    assert!(matches!(event, event::Fetched::Completed { rid, .. } if rid == repo_2));
    assert!(state.get_active_fetch(&repo_1).is_some());
    assert!(state.get_active_fetch(&repo_2).is_none());
    assert!(state.get_active_fetch(&repo_3).is_some());
}

#[test]
fn non_existent_returns_not_found() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);

    let event = state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });

    assert_eq!(
        event,
        event::Fetched::NotFound {
            from: node_a,
            rid: repo_1,
        }
    );
}

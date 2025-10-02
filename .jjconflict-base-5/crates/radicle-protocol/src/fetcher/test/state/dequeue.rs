use std::time::Duration;

use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::command;
use crate::fetcher::test::state::helpers;
use crate::fetcher::{FetchConfig, FetcherState};

#[test]
fn cannot_dequeue_while_node_at_capacity() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let repo_2: RepoId = arbitrary::r#gen(1);
    let refs_2 = helpers::gen_refs(3);
    let timeout_2 = Duration::from_secs(42);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: helpers::gen_refs(1),
        config: FetchConfig::default().with_timeout(Duration::from_secs(10)),
    });

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs: refs_2.clone(),
        config: FetchConfig::default().with_timeout(timeout_2),
    });

    let result = state.dequeue(&node_a);
    assert!(result.is_none());

    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });

    let result = state.dequeue(&node_a);
    let queued = result.unwrap();
    assert_eq!(queued.rid, repo_2);
    assert_eq!(queued.refs, refs_2);
    assert_eq!(queued.config.timeout(), timeout_2);
}

#[test]
fn maintains_fifo_order() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let repo_2: RepoId = arbitrary::r#gen(1);
    let repo_3: RepoId = arbitrary::r#gen(1);
    let repo_4: RepoId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: helpers::gen_refs(1),
        config,
    });

    // Queue in order: repo_2, repo_3, repo_4
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
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_4,
        refs: helpers::gen_refs(1),
        config,
    });

    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    assert_eq!(state.dequeue(&node_a).unwrap().rid, repo_2);

    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_2,
    });
    assert_eq!(state.dequeue(&node_a).unwrap().rid, repo_3);

    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_3,
    });
    assert_eq!(state.dequeue(&node_a).unwrap().rid, repo_4);
    assert!(state.dequeue(&node_a).is_none());
}

#[test]
fn empty_queue_returns_none() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);

    assert!(state.dequeue(&node_a).is_none());
}

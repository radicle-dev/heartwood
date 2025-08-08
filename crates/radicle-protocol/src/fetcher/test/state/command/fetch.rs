use std::time::Duration;

use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::{ActiveFetch, FetcherState};

#[test]
fn fetch_start_first_fetch_for_node() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = helpers::gen_refs_at(2);
    let timeout = Duration::from_secs(30);

    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    assert_eq!(
        event,
        event::Fetch::Started {
            rid: repo_1,
            from: node_a,
            refs_at: refs_at_1.clone(),
            timeout,
        }
    );
    assert_eq!(
        state.get_active_fetch(&repo_1),
        Some(&ActiveFetch {
            from: node_a,
            refs_at: refs_at_1,
        })
    );
}

#[test]
fn fetch_different_repo_same_node_within_capacity() {
    let mut state = FetcherState::new(helpers::config(2, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    let event1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    assert!(matches!(event1, event::Fetch::Started { .. }));

    let event2 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    assert!(matches!(event2, event::Fetch::Started { rid, .. } if rid == repo_2));
    assert!(state.get_active_fetch(&repo_1).is_some());
    assert!(state.get_active_fetch(&repo_2).is_some());
}

#[test]
fn fetch_same_repo_different_nodes_queues_second() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let node_b: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = helpers::gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    let event1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });
    assert!(matches!(event1, event::Fetch::Started { .. }));

    // Same repo from different node - gets queued since repo_1 is already active
    let event2 = state.fetch(command::Fetch {
        from: node_b,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    assert!(
        matches!(event2, event::Fetch::Queued { rid, from } if rid == repo_1 && from == node_b)
    );
    // Only node_a's fetch is active
    let active = state.get_active_fetch(&repo_1);
    assert!(active.is_some());
    assert_eq!(*active.unwrap().from(), node_a);
}

#[test]
fn fetch_duplicate_returns_already_fetching() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = helpers::gen_refs_at(2);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    assert_eq!(
        event,
        event::Fetch::AlreadyFetching {
            rid: repo_1,
            from: node_a,
        }
    );
}

#[test]
fn fetch_same_repo_different_refs_enqueues() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = helpers::gen_refs_at(1);
    let refs_at_2 = helpers::gen_refs_at(2);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_2.clone(),
        timeout,
    });

    assert_eq!(
        event,
        event::Fetch::Queued {
            rid: repo_1,
            from: node_a,
        }
    );
}

#[test]
fn fetch_at_capacity_enqueues() {
    let mut state = FetcherState::new(helpers::config(1, 10));
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

    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    assert_eq!(
        event,
        event::Fetch::Queued {
            rid: repo_2,
            from: node_a,
        }
    );
    assert!(state.get_active_fetch(&repo_1).is_some());
    assert!(state.get_active_fetch(&repo_2).is_none());
}

#[test]
fn fetch_queue_rejected_capacity_reached() {
    let mut state = FetcherState::new(helpers::config(1, 2));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let repo_4: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    // Fill concurrency
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    // Fill queue (capacity 2)
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    // Exceed queue capacity
    let refs_at_4 = helpers::gen_refs_at(1);
    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_4,
        refs_at: refs_at_4.clone(),
        timeout,
    });

    assert_eq!(
        event,
        event::Fetch::QueueAtCapacity {
            rid: repo_4,
            from: node_a,
            refs_at: refs_at_4,
            timeout,
            capacity: 2,
        }
    );
}

#[test]
fn fetch_queue_merges_already_queued() {
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

    // Second fetch for same queued repo - should merge refs
    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: refs_at_2b.clone(),
        timeout,
    });

    // Returns Queued (merged)
    assert_eq!(
        event,
        event::Fetch::Queued {
            rid: repo_2,
            from: node_a,
        }
    );

    // Dequeue and verify refs were merged
    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    let queued = state.dequeue(&node_a).unwrap();
    assert_eq!(queued.rid, repo_2);
    // refs_at should contain both sets of refs
    assert_eq!(queued.refs_at.len(), refs_at_2a.len() + refs_at_2b.len());
}

#[test]
fn fetch_queue_merge_empty_refs_fetches_all() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let refs_at_2 = helpers::gen_refs_at(2);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    // Queue with specific refs
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: refs_at_2.clone(),
        timeout,
    });

    // Queue again with empty refs (fetch everything)
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: vec![],
        timeout,
    });

    // Dequeue and verify refs became empty (fetch all)
    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    let queued = state.dequeue(&node_a).unwrap();
    assert_eq!(queued.rid, repo_2);
    assert!(queued.refs_at.is_empty());
}

#[test]
fn fetch_queue_merge_takes_longer_timeout() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let short_timeout = Duration::from_secs(10);
    let long_timeout = Duration::from_secs(60);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout: short_timeout,
    });

    // Queue with short timeout
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout: short_timeout,
    });

    // Queue again with longer timeout
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout: long_timeout,
    });

    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });
    // Dequeue and verify timeout is the longer one
    let queued = state.dequeue(&node_a).unwrap();
    assert_eq!(queued.timeout, long_timeout);
}

#[test]
fn fetch_after_previous_completed() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = helpers::gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });
    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_1,
    });

    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    assert!(matches!(event, event::Fetch::Started { .. }));
}

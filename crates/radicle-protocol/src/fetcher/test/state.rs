use std::num::NonZeroUsize;
use std::time::Duration;

use radicle::node::NodeId;
use radicle::prelude::RepoId;
use radicle::storage::refs::RefsAt;
use radicle::test::arbitrary;

use crate::fetcher::state::command;
use crate::fetcher::state::event;
use crate::fetcher::state::{ActiveFetch, Config, FetcherState, MaxQueueSize};

fn config(max_concurrency: usize, max_queue_size: usize) -> Config {
    Config::new()
        .with_max_concurrency(NonZeroUsize::new(max_concurrency).unwrap())
        .with_max_capacity(MaxQueueSize::new(
            NonZeroUsize::new(max_queue_size).unwrap(),
        ))
}

fn gen_refs_at(count: usize) -> Vec<RefsAt> {
    (0..count).map(|_| arbitrary::gen(1)).collect()
}

// =============================================================================
// Fetch Command Tests
// =============================================================================

#[test]
fn fetch_start_first_fetch_for_node() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(2);
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
    let mut state = FetcherState::new(config(2, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    let event1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });
    assert!(matches!(event1, event::Fetch::Started { .. }));

    let event2 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });

    assert!(matches!(event2, event::Fetch::Started { rid, .. } if rid == repo_2));
    assert!(state.get_active_fetch(&repo_1).is_some());
    assert!(state.get_active_fetch(&repo_2).is_some());
}

#[test]
fn fetch_same_repo_different_nodes_queues_second() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let node_b: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(1);
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(2);
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(1);
    let refs_at_2 = gen_refs_at(2);
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });

    let event = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
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
    let mut state = FetcherState::new(config(1, 2));
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
        refs_at: gen_refs_at(1),
        timeout,
    });

    // Fill queue (capacity 2)
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: gen_refs_at(1),
        timeout,
    });

    // Exceed queue capacity
    let refs_at_4 = gen_refs_at(1);
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let refs_at_2a = gen_refs_at(1);
    let refs_at_2b = gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let refs_at_2 = gen_refs_at(2);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let short_timeout = Duration::from_secs(10);
    let long_timeout = Duration::from_secs(60);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout: short_timeout,
    });

    // Queue with short timeout
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout: short_timeout,
    });

    // Queue again with longer timeout
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
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
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(1);
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

// =============================================================================
// Fetched Command Tests
// =============================================================================

#[test]
fn fetched_complete_single_ongoing() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(2);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
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
            refs_at: refs_at_1,
        }
    );
    assert!(state.get_active_fetch(&repo_1).is_none());
}

#[test]
fn fetched_complete_then_dequeue_fifo() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let refs_at_2 = gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });

    // Queue repo_2 first, then repo_3
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: refs_at_2.clone(),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: gen_refs_at(1),
        timeout,
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
    assert_eq!(queued.from, node_a);
    assert_eq!(queued.refs_at, refs_at_2);
}

#[test]
fn fetched_complete_one_of_multiple() {
    let mut state = FetcherState::new(config(3, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: gen_refs_at(1),
        timeout,
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
fn fetched_non_existent_returns_not_found() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);

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

// =============================================================================
// Cancel Command Tests
// =============================================================================

#[test]
fn cancel_single_ongoing() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let refs_at_1 = gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: refs_at_1.clone(),
        timeout,
    });

    let event = state.cancel(command::Cancel { from: node_a });

    match event {
        event::Cancel::Canceled {
            from,
            active: ongoing,
            queued,
        } => {
            assert_eq!(from, node_a);
            assert_eq!(ongoing.len(), 1);
            assert_eq!(
                ongoing.get(&repo_1),
                Some(&ActiveFetch {
                    from: node_a,
                    refs_at: refs_at_1,
                })
            );
            assert!(queued.is_empty());
        }
        _ => panic!("Expected Canceled event"),
    }
    assert!(state.get_active_fetch(&repo_1).is_none());
}

#[test]
fn cancel_ongoing_and_queued() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: gen_refs_at(1),
        timeout,
    });

    let event = state.cancel(command::Cancel { from: node_a });

    match event {
        event::Cancel::Canceled {
            active: ongoing,
            queued,
            ..
        } => {
            assert_eq!(ongoing.len(), 1);
            assert!(ongoing.contains_key(&repo_1));
            assert_eq!(queued.len(), 2);
        }
        _ => panic!("Expected Canceled event"),
    }
}

#[test]
fn cancel_non_existent_returns_unexpected() {
    let mut state = FetcherState::new(config(1, 10));
    let node_unknown: NodeId = arbitrary::gen(1);

    let event = state.cancel(command::Cancel { from: node_unknown });

    assert_eq!(event, event::Cancel::Unexpected { from: node_unknown });
}

#[test]
fn cancel_one_node_others_unaffected() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let node_b: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_b,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });

    state.cancel(command::Cancel { from: node_a });

    assert!(state.get_active_fetch(&repo_1).is_none());
    assert!(state.get_active_fetch(&repo_2).is_some());
}

// =============================================================================
// Dequeue Tests
// =============================================================================

#[test]
fn cannot_dequeue_while_node_at_capacity() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let refs_at_2 = gen_refs_at(3);
    let timeout_2 = Duration::from_secs(42);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout: Duration::from_secs(10),
    });

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: refs_at_2.clone(),
        timeout: timeout_2,
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
    assert_eq!(queued.from, node_a);
    assert_eq!(queued.refs_at, refs_at_2);
    assert_eq!(queued.timeout, timeout_2);
}

#[test]
fn dequeue_maintains_fifo_order() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let repo_4: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });

    // Queue in order: repo_2, repo_3, repo_4
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_4,
        refs_at: gen_refs_at(1),
        timeout,
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
fn dequeue_empty_returns_none() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);

    assert!(state.dequeue(&node_a).is_none());
}

// =============================================================================
// Configuration Tests
// =============================================================================

#[test]
fn config_high_concurrency() {
    let mut state = FetcherState::new(config(100, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    for i in 0..100 {
        let repo: RepoId = arbitrary::gen(i + 1);
        let event = state.fetch(command::Fetch {
            from: node_a,
            rid: repo,
            refs_at: gen_refs_at(1),
            timeout,
        });
        assert!(
            matches!(event, event::Fetch::Started { .. }),
            "Fetch {} should start",
            i
        );
    }

    assert_eq!(
        state
            .active_fetches()
            .iter()
            .filter(|(_, f)| *f.from() == node_a)
            .count(),
        100
    );
}

#[test]
fn config_min_queue_size() {
    let mut state = FetcherState::new(config(1, 1));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });

    let event1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
        timeout,
    });
    assert!(matches!(event1, event::Fetch::Queued { .. }));

    let event2 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: gen_refs_at(1),
        timeout,
    });
    assert!(matches!(event2, event::Fetch::QueueAtCapacity { .. }));
}

// =============================================================================
// Invariant Tests
// =============================================================================

#[test]
fn invariant_queue_integrity_after_merge() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let refs_at_2a = gen_refs_at(1);
    let refs_at_2b = gen_refs_at(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
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

// =============================================================================
// Concurrent Operation Tests
// =============================================================================

#[test]
fn concurrent_interleaved_operations() {
    let mut state = FetcherState::new(config(1, 10));
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
        refs_at: gen_refs_at(1),
        timeout,
    });
    assert!(matches!(e1, event::Fetch::Started { .. }));

    // fetch(B, r2)
    let e2 = state.fetch(command::Fetch {
        from: node_b,
        rid: repo_2,
        refs_at: gen_refs_at(1),
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
        refs_at: gen_refs_at(1),
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
fn concurrent_fetched_then_cancel() {
    let mut state = FetcherState::new(config(2, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: gen_refs_at(1),
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

// =============================================================================
// Multi-Node Tests
// =============================================================================

#[test]
fn multinode_independent_queues() {
    let mut state = FetcherState::new(config(1, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let node_b: NodeId = arbitrary::gen(1);
    let repo_a_active: RepoId = arbitrary::gen(1);
    let repo_b_active: RepoId = arbitrary::gen(2);
    let repo_a_queued: RepoId = arbitrary::gen(10);
    let repo_b_queued: RepoId = arbitrary::gen(20);
    let timeout = Duration::from_secs(30);

    // Fill capacity for both nodes
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_a_active,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_b,
        rid: repo_b_active,
        refs_at: gen_refs_at(1),
        timeout,
    });

    // Queue for both
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_a_queued,
        refs_at: gen_refs_at(1),
        timeout,
    });
    state.fetch(command::Fetch {
        from: node_b,
        rid: repo_b_queued,
        refs_at: gen_refs_at(1),
        timeout,
    });

    // Dequeue from A doesn't affect B
    state.fetched(command::Fetched {
        from: node_a,
        rid: repo_a_active,
    });
    let a_item = state.dequeue(&node_a);
    assert_eq!(a_item.unwrap().rid, repo_a_queued);

    state.fetched(command::Fetched {
        from: node_b,
        rid: repo_b_active,
    });
    let b_item = state.dequeue(&node_b);
    assert_eq!(b_item.unwrap().rid, repo_b_queued);
}

#[test]
fn multinode_high_count() {
    let mut state = FetcherState::new(config(1, 10));
    let timeout = Duration::from_secs(30);

    for i in 0..100 {
        let node: NodeId = arbitrary::gen(i + 1);
        let repo: RepoId = arbitrary::gen(i + 1);
        let event = state.fetch(command::Fetch {
            from: node,
            rid: repo,
            refs_at: gen_refs_at(1),
            timeout,
        });
        assert!(matches!(event, event::Fetch::Started { .. }));
    }

    assert_eq!(state.active_fetches().len(), 100);
}

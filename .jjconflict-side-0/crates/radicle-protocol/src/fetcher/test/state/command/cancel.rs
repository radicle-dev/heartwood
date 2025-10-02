use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::{ActiveFetch, FetchConfig, FetcherState};

#[test]
fn single_ongoing() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let refs_1 = helpers::gen_refs(1);
    let config = FetchConfig::default();

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: refs_1.clone(),
        config,
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
                    refs: refs_1,
                })
            );
            assert!(queued.is_empty());
        }
        _ => panic!("Expected Canceled event"),
    }
    assert!(state.get_active_fetch(&repo_1).is_none());
}

#[test]
fn ongoing_and_queued() {
    let mut state = FetcherState::new(helpers::config(1, 10));
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
fn non_existent_returns_unexpected() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_unknown: NodeId = arbitrary::r#gen(1);

    let event = state.cancel(command::Cancel { from: node_unknown });

    assert_eq!(event, event::Cancel::Unexpected { from: node_unknown });
}

#[test]
fn cancellation_is_isolated() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let node_b: NodeId = arbitrary::r#gen(1);
    let repo_1: RepoId = arbitrary::r#gen(1);
    let repo_2: RepoId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs: helpers::gen_refs(1),
        config,
    });
    state.fetch(command::Fetch {
        from: node_b,
        rid: repo_2,
        refs: helpers::gen_refs(1),
        config,
    });

    state.cancel(command::Cancel { from: node_a });

    assert!(state.get_active_fetch(&repo_1).is_none());
    assert!(state.get_active_fetch(&repo_2).is_some());
}

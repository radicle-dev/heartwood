use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::{FetchConfig, FetcherState};

#[test]
fn independent_queues() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let node_b: NodeId = arbitrary::r#gen(1);
    let repo_a_active: RepoId = arbitrary::r#gen(1);
    let repo_b_active: RepoId = arbitrary::r#gen(2);
    let repo_a_queued: RepoId = arbitrary::r#gen(10);
    let repo_b_queued: RepoId = arbitrary::r#gen(20);
    let fetch_config = FetchConfig::default();

    // Fill capacity for both nodes
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_a_active,
        refs: helpers::gen_refs(1),
        config: fetch_config,
    });
    state.fetch(command::Fetch {
        from: node_b,
        rid: repo_b_active,
        refs: helpers::gen_refs(1),
        config: fetch_config,
    });

    // Queue for both
    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_a_queued,
        refs: helpers::gen_refs(1),
        config: fetch_config,
    });
    state.fetch(command::Fetch {
        from: node_b,
        rid: repo_b_queued,
        refs: helpers::gen_refs(1),
        config: fetch_config,
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
fn high_count() {
    let mut state = FetcherState::new(helpers::config(1, 10));
    let config = FetchConfig::default();

    for i in 0..100 {
        let node: NodeId = arbitrary::r#gen(i + 1);
        let repo: RepoId = arbitrary::r#gen(i + 1);
        let event = state.fetch(command::Fetch {
            from: node,
            rid: repo,
            refs: helpers::gen_refs(1),
            config,
        });
        assert!(matches!(event, event::Fetch::Started { .. }));
    }

    assert_eq!(state.active_fetches().len(), 100);
}

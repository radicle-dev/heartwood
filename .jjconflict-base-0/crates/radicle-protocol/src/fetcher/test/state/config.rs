use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::{FetchConfig, FetcherState};

#[test]
fn high_concurrency() {
    let mut state = FetcherState::new(helpers::config(100, 10));
    let node_a: NodeId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    for i in 0..100 {
        let repo: RepoId = arbitrary::r#gen(i + 1);
        let event = state.fetch(command::Fetch {
            from: node_a,
            rid: repo,
            refs: helpers::gen_refs(1),
            config,
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
fn min_queue_size() {
    let mut state = FetcherState::new(helpers::config(1, 1));
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

    let event1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs: helpers::gen_refs(1),
        config,
    });
    assert!(matches!(event1, event::Fetch::Queued { .. }));

    let event2 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs: helpers::gen_refs(1),
        config,
    });
    assert!(matches!(event2, event::Fetch::QueueAtCapacity { .. }));
}

use std::time::Duration;

use radicle::test::arbitrary;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{command, event};
use crate::fetcher::test::state::helpers;
use crate::fetcher::FetcherState;

#[test]
fn high_concurrency() {
    let mut state = FetcherState::new(helpers::config(100, 10));
    let node_a: NodeId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    for i in 0..100 {
        let repo: RepoId = arbitrary::gen(i + 1);
        let event = state.fetch(command::Fetch {
            from: node_a,
            rid: repo,
            refs_at: helpers::gen_refs_at(1),
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
fn min_queue_size() {
    let mut state = FetcherState::new(helpers::config(1, 1));
    let node_a: NodeId = arbitrary::gen(1);
    let repo_1: RepoId = arbitrary::gen(1);
    let repo_2: RepoId = arbitrary::gen(1);
    let repo_3: RepoId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    state.fetch(command::Fetch {
        from: node_a,
        rid: repo_1,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });

    let event1 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_2,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    assert!(matches!(event1, event::Fetch::Queued { .. }));

    let event2 = state.fetch(command::Fetch {
        from: node_a,
        rid: repo_3,
        refs_at: helpers::gen_refs_at(1),
        timeout,
    });
    assert!(matches!(event2, event::Fetch::QueueAtCapacity { .. }));
}

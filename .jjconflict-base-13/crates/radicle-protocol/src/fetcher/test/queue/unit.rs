use std::time::Duration;

use radicle::test::arbitrary;
use radicle_core::RepoId;

use crate::fetcher::FetchConfig;
use crate::fetcher::QueuedFetch;
use crate::fetcher::RefsToFetch;
use crate::fetcher::state::Enqueue;
use crate::fetcher::test::queue::helpers::*;

#[test]
fn zero_timeout_accepted() {
    let mut queue = create_queue(10);
    let config = FetchConfig::default().with_timeout(Duration::ZERO);
    let item = QueuedFetch {
        rid: arbitrary::r#gen(1),
        refs: RefsToFetch::All,
        config,
    };
    assert_eq!(queue.enqueue(item), Enqueue::Queued);
}

#[test]
fn max_timeout_accepted() {
    let mut queue = create_queue(10);
    let config = FetchConfig::default().with_timeout(Duration::MAX);
    let item = QueuedFetch {
        rid: arbitrary::r#gen(1),
        refs: RefsToFetch::All,
        config,
    };
    assert_eq!(queue.enqueue(item), Enqueue::Queued);
}

#[test]
fn empty_refs_items_can_be_equal() {
    let rid: RepoId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    let item1 = QueuedFetch {
        rid,
        refs: RefsToFetch::All,
        config,
    };
    let item2 = QueuedFetch {
        rid,
        refs: RefsToFetch::All,
        config,
    };

    assert_eq!(item1, item2);
}

#[test]
fn merge_preserves_position_in_queue() {
    let mut queue = create_queue(10);

    let rid_first: RepoId = arbitrary::r#gen(1);
    let rid_second: RepoId = arbitrary::r#gen(2);
    let rid_third: RepoId = arbitrary::r#gen(3);
    let config = FetchConfig::default();

    // Enqueue three items
    let _ = queue.enqueue(QueuedFetch {
        rid: rid_first,
        refs: RefsToFetch::All,
        config: config.with_timeout(Duration::from_secs(30)),
    });
    let _ = queue.enqueue(QueuedFetch {
        rid: rid_second,
        refs: RefsToFetch::All,
        config: config.with_timeout(Duration::from_secs(30)),
    });
    let _ = queue.enqueue(QueuedFetch {
        rid: rid_third,
        refs: RefsToFetch::All,
        config: config.with_timeout(Duration::from_secs(30)),
    });

    // Merge into the second item
    let result = queue.enqueue(QueuedFetch {
        rid: rid_second,
        refs: vec![arbitrary::r#gen(1)].into(),
        config: config.with_timeout(Duration::from_secs(60)),
    });
    assert_eq!(result, Enqueue::Merged);

    // Order should be preserved: first, second (merged), third
    assert_eq!(queue.dequeue().unwrap().rid, rid_first);
    assert_eq!(queue.dequeue().unwrap().rid, rid_second);
    assert_eq!(queue.dequeue().unwrap().rid, rid_third);
}

#[test]
fn capacity_takes_precedence_over_merge_for_new_items() {
    let mut queue = create_queue(2);

    // Fill to capacity with unique items
    let _ = queue.enqueue(create_fetch());
    let _ = queue.enqueue(create_fetch());

    // New item (different rid) should be rejected
    let new_item = create_fetch();
    match queue.enqueue(new_item.clone()) {
        Enqueue::CapacityReached(returned) => assert_eq!(returned, new_item),
        _ => panic!("Expected CapacityReached"),
    }
}

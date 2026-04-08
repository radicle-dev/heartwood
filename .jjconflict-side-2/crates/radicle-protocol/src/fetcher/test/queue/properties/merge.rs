use std::num::NonZeroUsize;
use std::time::Duration;

use qcheck_macros::quickcheck;
use radicle::storage::refs::RefsAt;
use radicle::test::arbitrary;
use radicle_core::RepoId;

use crate::fetcher::state::Enqueue;
use crate::fetcher::test::queue::helpers::*;
use crate::fetcher::{FetchConfig, RefsToFetch};
use crate::fetcher::{MaxQueueSize, Queue, QueuedFetch};

#[quickcheck]
fn same_rid_merges_anywhere_in_queue(max_size: MaxQueueSize, merge_index: usize) -> bool {
    if max_size.as_usize() < 2 {
        return true; // Need at least 2 slots to test properly
    }

    let mut queue = Queue::new(max_size);
    let items = unique_fetches(max_size.as_usize() - 1); // Leave room for potential new item

    for item in &items {
        let _ = queue.enqueue(item.clone());
    }

    if items.is_empty() {
        return true;
    }

    // Try to enqueue an item with same rid as one already in queue
    let target_index = merge_index % items.len();
    let same_rid_item = QueuedFetch {
        rid: items[target_index].rid,
        refs: vec![arbitrary::r#gen(1)].into(),
        config: FetchConfig::default(),
    };

    matches!(queue.enqueue(same_rid_item), Enqueue::Merged)
}

#[quickcheck]
fn combines_refs(base_refs_count: u8, merge_refs_count: u8) -> bool {
    let base_refs_count = (base_refs_count as usize) % 5;
    let merge_refs_count = (merge_refs_count as usize) % 5;

    let mut queue = create_queue(10);
    let config = FetchConfig::default();

    let rid: RepoId = arbitrary::r#gen(1);
    let base_refs: Vec<RefsAt> = (0..base_refs_count).map(|_| arbitrary::r#gen(1)).collect();
    let merge_refs: Vec<RefsAt> = (0..merge_refs_count).map(|_| arbitrary::r#gen(1)).collect();

    let base_item = QueuedFetch {
        rid,
        refs: base_refs.clone().into(),
        config: config.with_timeout(Duration::from_secs(30)),
    };

    let merge_item = QueuedFetch {
        rid,
        refs: merge_refs.clone().into(),
        config: config.with_timeout(Duration::from_secs(30)),
    };

    let _ = queue.enqueue(base_item);
    let result = queue.enqueue(merge_item);

    if result != Enqueue::Merged {
        return false;
    }

    let dequeued = queue.dequeue().unwrap();

    // If either was empty, result should be empty (fetch everything)
    if base_refs.is_empty() || merge_refs.is_empty() {
        dequeued.refs == RefsToFetch::All
    } else {
        // Otherwise refs should be combined
        dequeued.refs.len() == Some(NonZeroUsize::new(base_refs_count + merge_refs_count).unwrap())
    }
}

#[quickcheck]
fn empty_refs_fetches_all() -> bool {
    let mut queue = create_queue(10);
    let rid: RepoId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    // First enqueue with specific refs
    let item_with_refs = QueuedFetch {
        rid,
        refs: vec![arbitrary::r#gen(1), arbitrary::r#gen(1)].into(),
        config,
    };

    // Second enqueue with empty refs (fetch everything)
    let item_empty_refs = QueuedFetch {
        rid,
        refs: RefsToFetch::All,
        config,
    };

    let _ = queue.enqueue(item_with_refs);
    let _ = queue.enqueue(item_empty_refs);

    let dequeued = queue.dequeue().unwrap();
    dequeued.refs == RefsToFetch::All // Should fetch everything
}

#[quickcheck]
fn longer_timeout_preserved(short_secs: u16, long_secs: u16) -> bool {
    let short = Duration::from_secs(short_secs.min(long_secs) as u64);
    let long = Duration::from_secs(short_secs.max(long_secs) as u64);
    let config = FetchConfig::default();
    let mut queue = create_queue(10);
    let rid: RepoId = arbitrary::r#gen(1);

    let item_short = QueuedFetch {
        rid,
        refs: RefsToFetch::All,
        config: config.with_timeout(short),
    };

    let item_long = QueuedFetch {
        rid,
        refs: RefsToFetch::All,
        config: config.with_timeout(long),
    };

    // Test both orderings
    let _ = queue.enqueue(item_short.clone());
    let _ = queue.enqueue(item_long.clone());
    let dequeued1 = queue.dequeue().unwrap();

    let mut queue2 = create_queue(10);
    let _ = queue2.enqueue(item_long);
    let _ = queue2.enqueue(item_short);
    let dequeued2 = queue2.dequeue().unwrap();

    dequeued1.config.timeout() == long && dequeued2.config.timeout() == long
}

#[quickcheck]
fn does_not_increase_queue_length() -> bool {
    let mut queue = create_queue(10);
    let rid: RepoId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    let item1 = QueuedFetch {
        rid,
        refs: vec![arbitrary::r#gen(1)].into(),
        config: config.with_timeout(Duration::from_secs(30)),
    };

    let item2 = QueuedFetch {
        rid,
        refs: vec![arbitrary::r#gen(1)].into(),
        config: config.with_timeout(Duration::from_secs(60)),
    };

    let _ = queue.enqueue(item1);
    let len_after_first = queue.len();

    let _ = queue.enqueue(item2);
    let len_after_merge = queue.len();

    len_after_first == 1 && len_after_merge == 1
}

#[quickcheck]
fn different_rid_accepted(base_item: QueuedFetch) -> bool {
    let mut queue = create_queue(10);
    let _ = queue.enqueue(base_item.clone());

    // Item with different rid should be queued (not merged)
    let different_rid = QueuedFetch {
        rid: arbitrary::r#gen(1),
        ..base_item
    };

    queue.enqueue(different_rid) == Enqueue::Queued
}

#[quickcheck]
fn succeed_when_at_capacity() -> bool {
    // When queue is at capacity, merging with existing item should still work
    let mut queue = create_queue(2);
    let rid: RepoId = arbitrary::r#gen(1);
    let config = FetchConfig::default();

    let item1 = QueuedFetch {
        rid,
        refs: RefsToFetch::All,
        config: config.with_timeout(Duration::from_secs(30)),
    };

    let item2 = QueuedFetch {
        rid: arbitrary::r#gen(1), // Different rid
        refs: RefsToFetch::All,
        config: config.with_timeout(Duration::from_secs(30)),
    };

    let merge_item = QueuedFetch {
        rid, // Same as item1
        refs: vec![arbitrary::r#gen(1)].into(),
        config: config.with_timeout(Duration::from_secs(60)),
    };

    let _ = queue.enqueue(item1);
    let _ = queue.enqueue(item2);

    // Queue is now at capacity, but merge should still work
    queue.enqueue(merge_item) == Enqueue::Merged
}

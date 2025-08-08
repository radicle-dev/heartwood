use std::num::NonZeroUsize;
use std::time::Duration;

use qcheck::Arbitrary;
use qcheck_macros::quickcheck;

use radicle::node::NodeId;
use radicle::prelude::RepoId;
use radicle::storage::refs::RefsAt;
use radicle::test::arbitrary;

use crate::fetcher::state::{Enqueue, MaxQueueSize, Queue, QueuedFetch};

impl Arbitrary for QueuedFetch {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        // Limit refs_at size to avoid slow shrinking
        let refs_at_len = usize::arbitrary(g) % 4;
        let refs_at: Vec<RefsAt> = (0..refs_at_len).map(|_| RefsAt::arbitrary(g)).collect();

        QueuedFetch {
            rid: RepoId::arbitrary(g),
            from: NodeId::arbitrary(g),
            refs_at,
            timeout: Duration::from_secs(u64::arbitrary(g) % 3600),
        }
    }
}

impl Arbitrary for MaxQueueSize {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        // Generate sizes between 1 and 256 for reasonable test performance
        let size = (usize::arbitrary(g) % 255) + 1;
        MaxQueueSize::new(NonZeroUsize::new(size).unwrap())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn create_queue(capacity: usize) -> Queue {
    Queue::new(MaxQueueSize::new(
        NonZeroUsize::new(capacity).expect("capacity must be non-zero"),
    ))
}

fn create_fetch() -> QueuedFetch {
    QueuedFetch {
        rid: arbitrary::gen(1),
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    }
}

/// Generate a vector of unique QueuedFetch items (unique by rid)
fn unique_fetches(count: usize) -> Vec<QueuedFetch> {
    (0..count).map(|_| create_fetch()).collect()
}

// ============================================================================
// FIFO Ordering Properties
// ============================================================================

#[quickcheck]
fn prop_fifo_ordering(count: u8) -> bool {
    let count = (count as usize) % 50; // Reasonable upper bound
    if count == 0 {
        return true;
    }

    let items = unique_fetches(count);
    let mut queue = create_queue(count);

    // Enqueue all items
    for item in &items {
        if queue.enqueue(item.clone()) != Enqueue::Queued {
            return false;
        }
    }

    // Dequeue and verify order
    for expected in items {
        match queue.dequeue() {
            Some(actual) if actual.rid == expected.rid => continue,
            _ => return false,
        }
    }

    queue.is_empty()
}

#[quickcheck]
fn prop_fifo_with_interleaved_operations(ops: Vec<bool>) -> bool {
    // Limit operations to avoid slow tests
    let ops: Vec<_> = ops.into_iter().take(100).collect();
    let capacity = ops.len().max(1);

    let mut queue = create_queue(capacity);
    let mut expected_order: Vec<QueuedFetch> = Vec::new();
    let mut dequeue_index = 0;

    for op in ops {
        if op {
            // Enqueue
            let item = create_fetch();
            match queue.enqueue(item.clone()) {
                Enqueue::Queued => expected_order.push(item),
                Enqueue::CapacityReached(_) => {} // Expected when full
                Enqueue::Merged => {}             // Can happen if same rid generated
            }
        } else {
            // Dequeue
            match queue.dequeue() {
                Some(item) => {
                    if dequeue_index >= expected_order.len()
                        || item.rid != expected_order[dequeue_index].rid
                    {
                        return false;
                    }
                    dequeue_index += 1;
                }
                None => {
                    // Should only happen if we've dequeued everything we enqueued
                    if dequeue_index != expected_order.len() {
                        return false;
                    }
                }
            }
        }
    }
    true
}

// ============================================================================
// Capacity Properties
// ============================================================================

#[quickcheck]
fn prop_bounded_capacity(max_size: MaxQueueSize, num_enqueues: u8) -> bool {
    let mut queue = Queue::new(max_size);

    for _ in 0..num_enqueues {
        let _ = queue.enqueue(create_fetch());

        // Invariant: length never exceeds capacity
        if queue.len() > max_size.as_usize() {
            return false;
        }
    }
    true
}

#[quickcheck]
fn prop_capacity_rejection(max_size: MaxQueueSize) -> bool {
    let mut queue = Queue::new(max_size);

    // Fill to capacity with unique items
    let items = unique_fetches(max_size.as_usize());
    for item in &items {
        if queue.enqueue(item.clone()) != Enqueue::Queued {
            return false;
        }
    }

    // Next enqueue of a NEW item must be rejected
    matches!(queue.enqueue(create_fetch()), Enqueue::CapacityReached(_))
}

#[quickcheck]
fn prop_capacity_restored_after_dequeue(max_size: MaxQueueSize, dequeue_count: u8) -> bool {
    let mut queue = Queue::new(max_size);

    // Fill to capacity
    for _ in 0..max_size.as_usize() {
        let _ = queue.enqueue(create_fetch());
    }

    // Dequeue some items
    let to_dequeue = (dequeue_count as usize).min(max_size.as_usize());
    for _ in 0..to_dequeue {
        let _ = queue.dequeue();
    }

    // Should be able to enqueue exactly that many items again
    for _ in 0..to_dequeue {
        if queue.enqueue(create_fetch()) != Enqueue::Queued {
            return false;
        }
    }

    // Next enqueue should fail
    matches!(queue.enqueue(create_fetch()), Enqueue::CapacityReached(_))
}

#[quickcheck]
fn prop_capacity_reached_returns_same_item(item: QueuedFetch) -> bool {
    let mut queue = create_queue(1);
    let _ = queue.enqueue(create_fetch()); // Fill the queue

    match queue.enqueue(item.clone()) {
        Enqueue::CapacityReached(returned) => returned == item,
        Enqueue::Merged => true, // If same rid, merge takes precedence
        _ => false,
    }
}

// ============================================================================
// Merge Properties (replacing duplicate rejection)
// ============================================================================

#[quickcheck]
fn prop_same_rid_merges_anywhere_in_queue(max_size: MaxQueueSize, merge_index: usize) -> bool {
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
        from: arbitrary::gen(1), // Different from
        refs_at: vec![arbitrary::gen(1)],
        timeout: Duration::from_secs(60),
    };

    matches!(queue.enqueue(same_rid_item), Enqueue::Merged)
}

#[quickcheck]
fn prop_merge_combines_refs(base_refs_count: u8, merge_refs_count: u8) -> bool {
    let base_refs_count = (base_refs_count as usize) % 5;
    let merge_refs_count = (merge_refs_count as usize) % 5;

    let mut queue = create_queue(10);

    let rid: RepoId = arbitrary::gen(1);
    let base_refs: Vec<RefsAt> = (0..base_refs_count).map(|_| arbitrary::gen(1)).collect();
    let merge_refs: Vec<RefsAt> = (0..merge_refs_count).map(|_| arbitrary::gen(1)).collect();

    let base_item = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: base_refs.clone(),
        timeout: Duration::from_secs(30),
    };

    let merge_item = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: merge_refs.clone(),
        timeout: Duration::from_secs(30),
    };

    let _ = queue.enqueue(base_item);
    let result = queue.enqueue(merge_item);

    if result != Enqueue::Merged {
        return false;
    }

    let dequeued = queue.dequeue().unwrap();

    // If either was empty, result should be empty (fetch everything)
    if base_refs.is_empty() || merge_refs.is_empty() {
        dequeued.refs_at.is_empty()
    } else {
        // Otherwise refs should be combined
        dequeued.refs_at.len() == base_refs_count + merge_refs_count
    }
}

#[quickcheck]
fn prop_merge_empty_refs_fetches_all() -> bool {
    let mut queue = create_queue(10);
    let rid: RepoId = arbitrary::gen(1);

    // First enqueue with specific refs
    let item_with_refs = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![arbitrary::gen(1), arbitrary::gen(1)],
        timeout: Duration::from_secs(30),
    };

    // Second enqueue with empty refs (fetch everything)
    let item_empty_refs = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    };

    let _ = queue.enqueue(item_with_refs);
    let _ = queue.enqueue(item_empty_refs);

    let dequeued = queue.dequeue().unwrap();
    dequeued.refs_at.is_empty() // Should fetch everything
}

#[quickcheck]
fn prop_merge_takes_longer_timeout(short_secs: u16, long_secs: u16) -> bool {
    let short = Duration::from_secs(short_secs.min(long_secs) as u64);
    let long = Duration::from_secs(short_secs.max(long_secs) as u64);

    let mut queue = create_queue(10);
    let rid: RepoId = arbitrary::gen(1);

    let item_short = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: short,
    };

    let item_long = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: long,
    };

    // Test both orderings
    let _ = queue.enqueue(item_short.clone());
    let _ = queue.enqueue(item_long.clone());
    let dequeued1 = queue.dequeue().unwrap();

    let mut queue2 = create_queue(10);
    let _ = queue2.enqueue(item_long);
    let _ = queue2.enqueue(item_short);
    let dequeued2 = queue2.dequeue().unwrap();

    dequeued1.timeout == long && dequeued2.timeout == long
}

#[quickcheck]
fn prop_merge_does_not_increase_queue_length() -> bool {
    let mut queue = create_queue(10);
    let rid: RepoId = arbitrary::gen(1);

    let item1 = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![arbitrary::gen(1)],
        timeout: Duration::from_secs(30),
    };

    let item2 = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![arbitrary::gen(1)],
        timeout: Duration::from_secs(60),
    };

    let _ = queue.enqueue(item1);
    let len_after_first = queue.len();

    let _ = queue.enqueue(item2);
    let len_after_merge = queue.len();

    len_after_first == 1 && len_after_merge == 1
}

#[quickcheck]
fn prop_different_rid_accepted(base_item: QueuedFetch) -> bool {
    let mut queue = create_queue(10);
    let _ = queue.enqueue(base_item.clone());

    // Item with different rid should be queued (not merged)
    let different_rid = QueuedFetch {
        rid: arbitrary::gen(1),
        ..base_item
    };

    queue.enqueue(different_rid) == Enqueue::Queued
}

#[quickcheck]
fn prop_merge_at_capacity_succeeds() -> bool {
    // When queue is at capacity, merging with existing item should still work
    let mut queue = create_queue(2);
    let rid: RepoId = arbitrary::gen(1);

    let item1 = QueuedFetch {
        rid,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    };

    let item2 = QueuedFetch {
        rid: arbitrary::gen(1), // Different rid
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    };

    let merge_item = QueuedFetch {
        rid, // Same as item1
        from: arbitrary::gen(1),
        refs_at: vec![arbitrary::gen(1)],
        timeout: Duration::from_secs(60),
    };

    let _ = queue.enqueue(item1);
    let _ = queue.enqueue(item2);

    // Queue is now at capacity, but merge should still work
    queue.enqueue(merge_item) == Enqueue::Merged
}

// ============================================================================
// Dequeue Properties
// ============================================================================

#[quickcheck]
fn prop_dequeue_enables_reenqueue(count: u8) -> bool {
    let count = ((count as usize) % 20).max(1);
    let items = unique_fetches(count);

    let mut queue = create_queue(count); // Exact capacity

    for item in &items {
        let _ = queue.enqueue(item.clone());
    }

    // Queue is full, dequeue first item
    let dequeued = queue.dequeue();
    if dequeued.is_none() {
        return false;
    }

    // Should be able to enqueue a new item now
    queue.enqueue(create_fetch()) == Enqueue::Queued
}

#[quickcheck]
fn prop_empty_dequeue_returns_none(max_size: MaxQueueSize, dequeue_attempts: u8) -> bool {
    let mut queue = Queue::new(max_size);

    // Multiple dequeues from empty queue should all return None
    for _ in 0..dequeue_attempts {
        if queue.dequeue().is_some() {
            return false;
        }
    }
    true
}

#[quickcheck]
fn prop_drained_queue_returns_none(max_size: MaxQueueSize, fill_count: u8) -> bool {
    let mut queue = Queue::new(max_size);
    let fill = (fill_count as usize).min(max_size.as_usize());

    // Fill then drain
    for _ in 0..fill {
        let _ = queue.enqueue(create_fetch());
    }
    for _ in 0..fill {
        let _ = queue.dequeue();
    }

    // Should return None now
    queue.dequeue().is_none()
}

// ============================================================================
// Equality Properties
// ============================================================================

#[quickcheck]
fn prop_equality_reflexive(item: QueuedFetch) -> bool {
    item == item.clone()
}

#[quickcheck]
fn prop_equality_symmetric(a: QueuedFetch, b: QueuedFetch) -> bool {
    (a == b) == (b == a)
}

#[quickcheck]
fn prop_equality_transitive(a: QueuedFetch, b: QueuedFetch, c: QueuedFetch) -> bool {
    if a == b && b == c {
        a == c
    } else {
        true
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[test]
fn unit_zero_timeout_accepted() {
    let mut queue = create_queue(10);
    let item = QueuedFetch {
        rid: arbitrary::gen(1),
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::ZERO,
    };
    assert_eq!(queue.enqueue(item), Enqueue::Queued);
}

#[test]
fn unit_max_timeout_accepted() {
    let mut queue = create_queue(10);
    let item = QueuedFetch {
        rid: arbitrary::gen(1),
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::MAX,
    };
    assert_eq!(queue.enqueue(item), Enqueue::Queued);
}

#[test]
fn unit_empty_refs_at_items_can_be_equal() {
    let rid: RepoId = arbitrary::gen(1);
    let from: NodeId = arbitrary::gen(1);
    let timeout = Duration::from_secs(30);

    let item1 = QueuedFetch {
        rid,
        from,
        refs_at: vec![],
        timeout,
    };
    let item2 = QueuedFetch {
        rid,
        from,
        refs_at: vec![],
        timeout,
    };

    assert_eq!(item1, item2);
}

#[test]
fn unit_merge_preserves_position_in_queue() {
    let mut queue = create_queue(10);

    let rid_first: RepoId = arbitrary::gen(1);
    let rid_second: RepoId = arbitrary::gen(2);
    let rid_third: RepoId = arbitrary::gen(3);

    // Enqueue three items
    let _ = queue.enqueue(QueuedFetch {
        rid: rid_first,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    });
    let _ = queue.enqueue(QueuedFetch {
        rid: rid_second,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    });
    let _ = queue.enqueue(QueuedFetch {
        rid: rid_third,
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    });

    // Merge into the second item
    let result = queue.enqueue(QueuedFetch {
        rid: rid_second,
        from: arbitrary::gen(1),
        refs_at: vec![arbitrary::gen(1)],
        timeout: Duration::from_secs(60),
    });
    assert_eq!(result, Enqueue::Merged);

    // Order should be preserved: first, second (merged), third
    assert_eq!(queue.dequeue().unwrap().rid, rid_first);
    assert_eq!(queue.dequeue().unwrap().rid, rid_second);
    assert_eq!(queue.dequeue().unwrap().rid, rid_third);
}

#[test]
fn unit_capacity_takes_precedence_over_merge_for_new_items() {
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

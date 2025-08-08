use qcheck_macros::quickcheck;

use crate::fetcher::test::queue::helpers::*;
use crate::fetcher::{state::Enqueue, MaxQueueSize};
use crate::fetcher::{Queue, QueuedFetch};

#[quickcheck]
fn bounded(max_size: MaxQueueSize, num_enqueues: u8) -> bool {
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
fn rejection(max_size: MaxQueueSize) -> bool {
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
fn restored_after_dequeue(max_size: MaxQueueSize, dequeue_count: u8) -> bool {
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
fn capacity_reached_returns_same_item(item: QueuedFetch) -> bool {
    let mut queue = create_queue(1);
    let _ = queue.enqueue(create_fetch()); // Fill the queue

    match queue.enqueue(item.clone()) {
        Enqueue::CapacityReached(returned) => returned == item,
        Enqueue::Merged => true, // If same rid, merge takes precedence
        _ => false,
    }
}

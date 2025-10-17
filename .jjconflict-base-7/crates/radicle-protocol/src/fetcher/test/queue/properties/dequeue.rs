use qcheck_macros::quickcheck;

use crate::fetcher::state::Enqueue;
use crate::fetcher::test::queue::helpers::*;
use crate::fetcher::{MaxQueueSize, Queue};

#[quickcheck]
fn enables_reenqueue(count: u8) -> bool {
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
fn empty_queue_returns_none(max_size: MaxQueueSize, dequeue_attempts: u8) -> bool {
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
fn drained_queue_returns_none(max_size: MaxQueueSize, fill_count: u8) -> bool {
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

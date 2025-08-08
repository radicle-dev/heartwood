use qcheck_macros::quickcheck;

use crate::fetcher::state::Enqueue;
use crate::fetcher::test::queue::helpers::*;
use crate::fetcher::QueuedFetch;

#[quickcheck]
fn ordering(count: u8) -> bool {
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
fn interleaved_operations(ops: Vec<bool>) -> bool {
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

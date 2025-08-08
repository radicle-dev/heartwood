use std::{num::NonZeroUsize, time::Duration};

use radicle::test::arbitrary;

use crate::fetcher::{MaxQueueSize, Queue, QueuedFetch};

pub fn create_queue(capacity: usize) -> Queue {
    Queue::new(MaxQueueSize::new(
        NonZeroUsize::new(capacity).expect("capacity must be non-zero"),
    ))
}

pub fn create_fetch() -> QueuedFetch {
    QueuedFetch {
        rid: arbitrary::gen(1),
        from: arbitrary::gen(1),
        refs_at: vec![],
        timeout: Duration::from_secs(30),
    }
}

/// Generate a vector of unique QueuedFetch items (unique by rid)
pub fn unique_fetches(count: usize) -> Vec<QueuedFetch> {
    (0..count).map(|_| create_fetch()).collect()
}

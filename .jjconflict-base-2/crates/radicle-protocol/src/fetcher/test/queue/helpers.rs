use std::num::NonZeroUsize;

use radicle::test::arbitrary;

use crate::fetcher::{FetchConfig, MaxQueueSize, Queue, QueuedFetch, RefsToFetch};

pub fn create_queue(capacity: usize) -> Queue {
    Queue::new(MaxQueueSize::new(
        NonZeroUsize::new(capacity).expect("capacity must be non-zero"),
    ))
}

pub fn create_fetch() -> QueuedFetch {
    QueuedFetch {
        rid: arbitrary::r#gen(1),
        refs: RefsToFetch::All,
        config: FetchConfig::default(),
    }
}

/// Generate a vector of unique QueuedFetch items (unique by rid)
pub fn unique_fetches(count: usize) -> Vec<QueuedFetch> {
    (0..count).map(|_| create_fetch()).collect()
}

use std::num::NonZeroUsize;

use radicle::{storage::refs::RefsAt, test::arbitrary};

use crate::fetcher::{Config, MaxQueueSize};

pub fn config(max_concurrency: usize, max_queue_size: usize) -> Config {
    Config::new()
        .with_max_concurrency(NonZeroUsize::new(max_concurrency).unwrap())
        .with_max_capacity(MaxQueueSize::new(
            NonZeroUsize::new(max_queue_size).unwrap(),
        ))
}

pub fn gen_refs_at(count: usize) -> Vec<RefsAt> {
    (0..count).map(|_| arbitrary::gen(1)).collect()
}

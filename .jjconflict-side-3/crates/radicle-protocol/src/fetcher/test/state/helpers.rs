use std::num::NonZeroUsize;

use radicle::test::arbitrary;

use crate::fetcher::{Config, MaxQueueSize, RefsToFetch};

pub fn config(max_concurrency: usize, max_queue_size: usize) -> Config {
    Config::new()
        .with_max_concurrency(NonZeroUsize::new(max_concurrency).unwrap())
        .with_max_capacity(MaxQueueSize::new(
            NonZeroUsize::new(max_queue_size).unwrap(),
        ))
}

pub fn gen_refs(count: usize) -> RefsToFetch {
    let refs: Vec<_> = (0..count).map(|_| arbitrary::r#gen(1)).collect();
    refs.into()
}

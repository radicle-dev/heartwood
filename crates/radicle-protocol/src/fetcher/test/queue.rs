mod helpers;
mod properties;
mod unit;

use std::num::NonZeroUsize;
use std::time::Duration;

use qcheck::Arbitrary;

use radicle::storage::refs::RefsAt;
use radicle_core::{NodeId, RepoId};

use crate::fetcher::state::{MaxQueueSize, QueuedFetch};

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
        let size = NonZeroUsize::MIN.saturating_add(usize::arbitrary(g) % 255);
        MaxQueueSize::new(size)
    }
}

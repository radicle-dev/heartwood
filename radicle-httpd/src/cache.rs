use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;
use tokio::sync::Mutex;

use radicle::prelude::RepoId;
use radicle_surf::Oid;

#[derive(Clone)]
pub struct Cache {
    pub tree: Arc<Mutex<LruCache<(RepoId, Oid, String), serde_json::Value>>>,
}

impl Cache {
    /// Creates a new cache of the given size.
    pub fn new(size: NonZeroUsize) -> Self {
        Cache {
            tree: Arc::new(Mutex::new(LruCache::new(size))),
        }
    }
}

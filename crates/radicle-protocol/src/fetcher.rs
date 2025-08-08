pub mod state;
pub use state::{ActiveFetch, Config, FetcherState, MaxQueueSize, Queue, QueueIter, QueuedFetch};

#[cfg(test)]
mod test;

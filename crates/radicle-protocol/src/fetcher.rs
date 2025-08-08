pub mod service;
pub use service::FetcherService;

pub mod state;
pub use state::{ActiveFetch, Config, FetcherState, MaxQueueSize, Queue, QueueIter, QueuedFetch};

#[cfg(test)]
mod test;

// TODO(finto): `Service::fetch_refs_at` and the use of `refs_status_of` is a
// layer above the `Fetcher` where it would perform I/O, mocked out by a trait,
// to check if there are wants and add a fetch to the Fetcher.

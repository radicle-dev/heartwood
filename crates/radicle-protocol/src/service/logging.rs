use log::Level;

use radicle::node::NodeId;
use radicle::prelude::RepoId;

use crate::fetcher;

pub fn fetch_command(
    fetcher::state::command::Fetch {
        from, rid, refs_at, ..
    }: &fetcher::state::command::Fetch,
) {
    if log::log_enabled!(Level::Trace) {
        log::trace!(target: "service", "Fetch command repo={rid} from={from} refs_at={refs_at:?}");
    } else {
        log::debug!(target: "service", "Fetch command repo={rid} from={from}");
    }
}

pub fn fetch_event(event: &fetcher::state::event::Fetch) {
    match event {
        fetcher::state::event::Fetch::Started {
            rid,
            from,
            refs_at: _,
            timeout: _,
        } => {
            log::debug!(target: "service", "Fetch started repo={rid} from={from}");
        }
        fetcher::state::event::Fetch::AlreadyFetching { rid, from } => {
            log::debug!(target: "service", "Already fetching repo={rid} from={from}");
        }
        fetcher::state::event::Fetch::QueueAtCapacity {
            rid,
            from,
            capacity,
            refs_at: _,
            timeout: _,
        } => {
            log::debug!(target: "service", "Queue at capacity repo={rid} from={from} capacity={capacity}");
        }
        fetcher::state::event::Fetch::Queued { rid, from } => {
            log::debug!(target: "service", "Fetch queued repo={rid} from={from}");
        }
    }
}

pub fn fetched_event(
    event: &fetcher::state::event::Fetched,
    result: &Result<crate::worker::fetch::FetchResult, crate::worker::FetchError>,
) {
    match event {
        fetcher::state::event::Fetched::NotFound { from, rid } => {
            log::warn!(target: "service", "Unexpected fetch result repo={rid} from={from}");
            fetch_result(rid, from, result);
        }
        fetcher::state::event::Fetched::Completed {
            from,
            rid,
            refs_at: _,
        } => {
            log::debug!(target: "service", "Fetch completed repo={rid} from={from}");
            fetch_result(rid, from, result);
        }
    }
}

fn fetch_result(
    rid: &RepoId,
    from: &NodeId,
    result: &Result<crate::worker::fetch::FetchResult, crate::worker::FetchError>,
) {
    match result {
        Ok(inner) => {
            let fetch_kind = if inner.clone { "cloned" } else { "pulled" };
            let msg = format!("Repository {fetch_kind} successfully");
            log::info!(target: "service", "{msg} repo={rid} from={from}");
            if log::log_enabled!(Level::Trace) {
                log::trace!(target: "service", "Fetched references repo={rid} from={from} updated={:?}", inner.updated);
            } else if log::log_enabled!(Level::Debug) {
                log::trace!(target: "service", "Fetched references repo={rid} from={from} updated={:?}", inner.updated.iter().filter(|up| !up.is_skipped()).collect::<Vec<_>>())
            }
        }
        Err(err) => {
            log::warn!(target: "service", "Fetch failed repo={rid} from={from}: {err}");
        }
    }
}

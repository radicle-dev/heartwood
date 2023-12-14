use std::time::Duration;

use radicle::identity::Id;
use radicle::node;
use radicle::node::Handle as _;
use radicle::Node;

use crate::terminal as term;

pub fn sync(rid: Id, node: &mut Node) -> anyhow::Result<()> {
    match sync_(rid, node) {
        Ok(()) => Ok(()),
        Err(e) if e.is_connection_err() => {
            term::hint(format!(
                "Node is stopped, to announce changes to the network, start it with 'rad node start'"
            ));
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Sync with the network.
fn sync_(rid: Id, node: &mut Node) -> Result<(), radicle::node::Error> {
    let seeds = node.seeds(rid)?;
    let connected = seeds.connected().map(|s| s.nid).collect::<Vec<_>>();

    if connected.is_empty() {
        term::info!("Not connected to any seeds.");
        return Ok(());
    }

    let mut spinner = term::spinner(format!("Syncing with {} node(s)..", connected.len()));
    let result = node.announce(
        rid,
        connected,
        Duration::from_secs(9),
        |event| match event {
            node::AnnounceEvent::Announced => {}
            node::AnnounceEvent::RefsSynced { remote } => {
                spinner.message(format!("Synced with {remote}.."));
            }
        },
    )?;

    if result.synced.is_empty() {
        spinner.failed();
    } else {
        spinner.message(format!("Synced with {} node(s)", result.synced.len()));
        spinner.finish();
    }
    Ok(())
}

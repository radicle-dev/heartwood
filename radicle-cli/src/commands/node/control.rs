use radicle::node::{Address, Handle as _, NodeId};
use radicle::Node;

use crate::terminal as term;

pub fn start() -> anyhow::Result<()> {
    todo!()
}

pub fn stop(node: Node) -> anyhow::Result<()> {
    let spinner = term::spinner("Stopping the node...");
    if let Err(err) = node.shutdown() {
        spinner.error(format!("Error occurred while shutting down node: {err}"));
    } else {
        spinner.finish();
    }
    Ok(())
}

pub fn connect(node: &mut Node, nid: NodeId, addr: Address) -> anyhow::Result<()> {
    let spinner = term::spinner(format!(
        "Connecting to {}@{addr}...",
        term::format::node(&nid)
    ));
    if let Err(err) = node.connect(nid, addr.clone()) {
        spinner.error(format!(
            "Failed to connect to {}@{}: {}",
            term::format::node(&nid),
            term::format::secondary(addr),
            err,
        ))
    } else {
        spinner.finish()
    }
    Ok(())
}

pub fn status(node: &Node) {
    if node.is_running() {
        term::success!("The node is {}", term::format::positive("running"));
    } else {
        term::info!("The node is {}", term::format::negative("stopped"));
    }
}

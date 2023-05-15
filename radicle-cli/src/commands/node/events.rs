use std::time;

use radicle::node::Handle;

pub fn run(node: impl Handle) -> anyhow::Result<()> {
    let events = node.subscribe(time::Duration::MAX)?;
    for event in events {
        let event = event?;
        let json_string = serde_json::to_string(&event)?;
        println!("{json_string}");
    }

    Ok(())
}

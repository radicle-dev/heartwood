use std::time;

use radicle::node::Handle;

pub fn run(node: impl Handle, count: usize, timeout: time::Duration) -> anyhow::Result<()> {
    let events = node.subscribe(timeout)?;
    for (i, event) in events.enumerate() {
        let event = event?;
        let obj = serde_json::to_string(&event)?;

        println!("{obj}");

        // Only output up to `count` events.
        if i + 1 >= count {
            break;
        }
    }

    Ok(())
}

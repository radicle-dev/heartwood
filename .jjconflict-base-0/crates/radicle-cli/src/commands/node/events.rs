use std::time;

use radicle::node::{Event, Handle};

pub fn run<H>(node: H, count: usize, timeout: time::Duration) -> anyhow::Result<()>
where
    H: Handle<Event = Result<Event, <H as Handle>::Error>>,
{
    let events = node.subscribe(timeout)?;
    for (i, event) in events.into_iter().enumerate() {
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

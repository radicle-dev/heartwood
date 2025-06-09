use std::str::FromStr;

use radicle::node;
use radicle::node::device::Device;
use radicle::node::UserAgent;
use radicle::test::fixtures::gen;

use crate::test::arbitrary;
use crate::{
    prelude::{LocalDuration, LocalTime, Message},
    service::message::{InventoryAnnouncement, NodeAnnouncement},
    PROTOCOL_VERSION,
};

pub fn messages(count: usize, now: LocalTime, delta: LocalDuration) -> Vec<Message> {
    let mut rng = fastrand::Rng::new();
    let mut msgs = Vec::new();

    for _ in 0..count {
        let signer = Device::mock_rng(&mut rng);
        let time = if delta == LocalDuration::from_secs(0) {
            now
        } else {
            let delta = LocalDuration::from_secs(rng.u64(0..delta.as_secs()));

            if rng.bool() {
                now + delta
            } else {
                now - delta
            }
        };

        msgs.push(Message::node(
            NodeAnnouncement {
                version: PROTOCOL_VERSION,
                features: node::Features::SEED,
                timestamp: time.into(),
                alias: node::Alias::new(gen::string(5)),
                addresses: None.into(),
                nonce: 0,
                agent: UserAgent::from_str("/radicle:test/").unwrap(),
            }
            .solve(0)
            .unwrap(),
            &signer,
        ));
        msgs.push(Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::vec(3).try_into().unwrap(),
                timestamp: time.into(),
            },
            &signer,
        ));
    }
    msgs
}

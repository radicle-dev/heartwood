use localtime::{LocalDuration, LocalTime};
use protocol::service::message::{InventoryAnnouncement, Message, NodeAnnouncement};
use radicle::crypto::SigningKey;
use radicle::node;
use radicle::node::PROTOCOL_VERSION;
use radicle::node::UserAgent;
use radicle::test::arbitrary;
use radicle::test::fixtures::r#gen;

use crate::test::arbitrary;

pub fn messages(count: usize, now: LocalTime, delta: LocalDuration) -> Vec<Message> {
    let mut rng = fastrand::Rng::new();
    let mut msgs = Vec::new();

    for _ in 0..count {
        let signer = SigningKey::mock(rng.usize(0x1000..0xFFFF));
        let time = if delta == LocalDuration::from_secs(0) {
            now
        } else {
            let delta = LocalDuration::from_secs(rng.u64(0..delta.as_secs()));

            if rng.bool() { now + delta } else { now - delta }
        };

        msgs.push(Message::node(
            NodeAnnouncement {
                version: PROTOCOL_VERSION,
                features: node::Features::SEED,
                timestamp: time.into(),
                alias: node::Alias::new(r#gen::string(5)),
                addresses: None.into(),
                nonce: 0,
                agent: UserAgent::test(),
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

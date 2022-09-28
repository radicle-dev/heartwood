use radicle::test::signer::MockSigner;

use crate::test::arbitrary;
use crate::{
    prelude::{LocalDuration, LocalTime, Message},
    service::message::InventoryAnnouncement,
};

pub fn messages(count: usize, now: LocalTime, delta: LocalDuration) -> Vec<Message> {
    let mut rng = fastrand::Rng::new();
    let mut msgs = Vec::new();

    for _ in 0..count {
        let signer = MockSigner::new(&mut rng);
        let delta = LocalDuration::from_secs(rng.u64(0..delta.as_secs()));
        let time = if rng.bool() { now + delta } else { now - delta };

        msgs.push(Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::gen(3),
                timestamp: time.as_secs(),
            },
            signer,
        ))
    }
    msgs
}

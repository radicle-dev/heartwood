use radicle::crypto::test::signer::MockSigner;

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

        msgs.push(Message::inventory(
            InventoryAnnouncement {
                inventory: arbitrary::vec(3).try_into().unwrap(),
                timestamp: time.as_secs(),
            },
            &signer,
        ))
    }
    msgs
}

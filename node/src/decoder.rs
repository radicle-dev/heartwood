use std::marker::PhantomData;

use crate::protocol::message::Envelope;
use serde::Deserialize;

/// Message stream decoder.
///
/// Used to for example turn a byte stream into network messages.
#[derive(Debug)]
pub struct Decoder<D = Envelope> {
    unparsed: Vec<u8>,
    item: PhantomData<D>,
}

impl<D> From<Vec<u8>> for Decoder<D> {
    fn from(unparsed: Vec<u8>) -> Self {
        Self {
            unparsed,
            item: PhantomData,
        }
    }
}

impl<'de, D: Deserialize<'de>> Decoder<D> {
    /// Create a new stream decoder.
    pub fn new(capacity: usize) -> Self {
        Self {
            unparsed: Vec::with_capacity(capacity),
            item: PhantomData,
        }
    }

    /// Input bytes into the decoder.
    pub fn input(&mut self, bytes: &[u8]) {
        self.unparsed.extend_from_slice(bytes);
    }

    /// Decode and return the next message. Returns [`None`] if nothing was decoded.
    pub fn decode_next(&mut self) -> Result<Option<D>, serde_json::Error> {
        let mut de = serde_json::Deserializer::from_reader(self.unparsed.as_slice()).into_iter();

        match de.next() {
            Some(Ok(msg)) => {
                self.unparsed.drain(..de.byte_offset());
                Ok(Some(msg))
            }
            Some(Err(err)) if err.is_eof() => Ok(None),

            result => result.transpose(),
        }
    }
}

impl<'de, D: Deserialize<'de>> Iterator for Decoder<D> {
    type Item = Result<D, serde_json::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.decode_next().transpose()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;

    const MSG_HELLO: &[u8] = b"{\"cmd\":\"hello\"}";
    const MSG_BYE: &[u8] = b"{\"cmd\":\"goodbye\"}";

    #[quickcheck]
    fn prop_decode_next(chunk_size: usize) {
        let mut bytes = vec![];
        let mut msgs = vec![];
        let mut decoder = Decoder::<serde_json::Value>::new(64);

        let chunk_size = 1 + chunk_size % MSG_HELLO.len() + MSG_BYE.len();

        bytes.extend_from_slice(MSG_HELLO);
        bytes.extend_from_slice(MSG_BYE);

        for chunk in bytes.as_slice().chunks(chunk_size) {
            decoder.input(chunk);

            while let Some(msg) = decoder.decode_next().unwrap() {
                msgs.push(msg);
            }
        }

        assert_eq!(decoder.unparsed.len(), 0);
        assert_eq!(msgs.len(), 2);
        assert_eq!(
            msgs[0],
            serde_json::json!({
                "cmd": "hello",
            })
        );
        assert_eq!(
            msgs[1],
            serde_json::json!({
                "cmd": "goodbye",
            })
        );
    }
}

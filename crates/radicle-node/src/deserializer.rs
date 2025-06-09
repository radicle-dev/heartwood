use std::io;
use std::marker::PhantomData;

use crate::bounded;
use crate::prelude::BoundedVec;
use crate::service::message::Message;
use crate::wire;

/// Message stream deserializer.
///
/// Used to for example turn a byte stream into network messages.
#[derive(Debug)]
pub struct Deserializer<const B: usize, D = Message> {
    unparsed: BoundedVec<u8, B>,
    item: PhantomData<D>,
}

impl<const B: usize, D: wire::Decode> Default for Deserializer<B, D> {
    fn default() -> Self {
        Self::new(wire::Size::MAX as usize + 1)
    }
}

impl<const B: usize, D> TryFrom<Vec<u8>> for Deserializer<B, D> {
    type Error = bounded::Error;

    fn try_from(unparsed: Vec<u8>) -> Result<Self, Self::Error> {
        BoundedVec::try_from(unparsed).map(|unparsed| Self {
            unparsed,
            item: PhantomData,
        })
    }
}

impl<const B: usize, D: wire::Decode> Deserializer<B, D> {
    /// Create a new stream decoder.
    pub fn new(capacity: usize) -> Self {
        Self {
            unparsed: BoundedVec::with_capacity(capacity)
                .expect("Deserializer::new: capacity exceeds maximum"),
            item: PhantomData,
        }
    }

    /// Input bytes into the decoder.
    pub fn input(&mut self, bytes: &[u8]) -> Result<(), bounded::Error> {
        self.unparsed.extend_from_slice(bytes)
    }

    /// Decode and return the next message. Returns [`None`] if nothing was decoded.
    pub fn deserialize_next(&mut self) -> Result<Option<D>, wire::Error> {
        let mut reader = io::Cursor::new(self.unparsed.as_slice());

        match D::decode(&mut reader) {
            Ok(msg) => {
                let pos = reader.position() as usize;
                self.unparsed.drain(..pos);

                Ok(Some(msg))
            }
            Err(err) if err.is_eof() => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Drain the unparsed buffer.
    pub fn unparsed(&mut self) -> impl ExactSizeIterator<Item = u8> + '_ {
        self.unparsed.drain(..)
    }

    /// Return whether there are unparsed bytes.
    pub fn is_empty(&self) -> bool {
        self.unparsed.is_empty()
    }

    /// Return the size of the unparsed data.
    pub fn len(&self) -> usize {
        self.unparsed.len()
    }
}

impl<const B: usize, D: wire::Decode> io::Write for Deserializer<B, D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.input(buf).map_err(|_| io::ErrorKind::OutOfMemory)?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<const B: usize, D: wire::Decode> Iterator for Deserializer<B, D> {
    type Item = Result<D, wire::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.deserialize_next().transpose()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use qcheck_macros::quickcheck;

    use crate::test::assert_matches;

    const MSG_HELLO: &[u8] = &[5, b'h', b'e', b'l', b'l', b'o'];
    const MSG_BYE: &[u8] = &[3, b'b', b'y', b'e'];

    #[test]
    fn test_decode_next() {
        let mut decoder = Deserializer::<1024, String>::new(8);

        decoder.input(&[3, b'b']).unwrap();
        assert_matches!(decoder.deserialize_next(), Ok(None));
        assert_eq!(decoder.unparsed.len(), 2);

        decoder.input(&[b'y']).unwrap();
        assert_matches!(decoder.deserialize_next(), Ok(None));
        assert_eq!(decoder.unparsed.len(), 3);

        decoder.input(&[b'e']).unwrap();
        assert_matches!(decoder.deserialize_next(), Ok(Some(s)) if s.as_str() == "bye");
        assert_eq!(decoder.unparsed.len(), 0);
        assert!(decoder.is_empty());
    }

    #[test]
    fn test_unparsed() {
        let mut decoder = Deserializer::<1024, String>::new(8);

        decoder.input(&[3, b'b', b'y']).unwrap();
        assert_eq!(decoder.unparsed().collect::<Vec<_>>(), vec![3, b'b', b'y']);
        assert!(decoder.is_empty());
    }

    #[quickcheck]
    fn prop_decode_next(chunk_size: usize) {
        let mut bytes = vec![];
        let mut msgs = vec![];
        let mut decoder = Deserializer::<1024, String>::new(8);

        let chunk_size = 1 + chunk_size % MSG_HELLO.len() + MSG_BYE.len();

        bytes.extend_from_slice(MSG_HELLO);
        bytes.extend_from_slice(MSG_BYE);

        for chunk in bytes.as_slice().chunks(chunk_size) {
            decoder.input(chunk).unwrap();

            while let Some(msg) = decoder.deserialize_next().unwrap() {
                msgs.push(msg);
            }
        }

        assert_eq!(decoder.unparsed.len(), 0);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], String::from("hello"));
        assert_eq!(msgs[1], String::from("bye"));
    }
}

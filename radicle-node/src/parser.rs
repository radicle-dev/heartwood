use std::io;
use std::marker::PhantomData;

use crate::service::message::Message;
use crate::wire;

/// Parser of Radicle messages from the network stream.
///
/// Used to turn a byte stream into network messages.
#[derive(Debug)]
pub struct Parser<D = Message> {
    unparsed: Vec<u8>,
    item: PhantomData<D>,
}

impl<D> From<Vec<u8>> for Parser<D> {
    fn from(unparsed: Vec<u8>) -> Self {
        Self {
            unparsed,
            item: PhantomData,
        }
    }
}

impl<D: wire::Decode> Parser<D> {
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
    pub fn decode_next(&mut self) -> Result<Option<D>, wire::Error> {
        let mut reader = io::Cursor::new(self.unparsed.as_mut_slice());

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
}

impl<D: wire::Decode> io::Write for Parser<D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.input(buf);

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<D: wire::Decode> Iterator for Parser<D> {
    type Item = Result<D, wire::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.decode_next().transpose()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;

    const MSG_HELLO: &[u8] = &[5, b'h', b'e', b'l', b'l', b'o'];
    const MSG_BYE: &[u8] = &[3, b'b', b'y', b'e'];

    #[quickcheck]
    fn prop_decode_next(chunk_size: usize) {
        let mut bytes = vec![];
        let mut msgs = vec![];
        let mut decoder = Parser::<String>::new(8);

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
        assert_eq!(msgs[0], String::from("hello"));
        assert_eq!(msgs[1], String::from("bye"));
    }
}

use std::convert::Infallible;

// TODO: Implement Try trait once stabilized
pub enum HandshakeResult<H: Handshake, T: Transcode> {
    Next(H, Vec<u8>),
    Complete(T, Vec<u8>),
    Error(H::Error),
}

pub trait Handshake: Sized {
    /// Errors which may happen during the handshake.
    type Error: std::error::Error;

    type Transcoder: Transcode;

    fn new() -> Self;
    fn next_stage(self, input: &[u8]) -> HandshakeResult<Self, Self::Transcoder>;
}

#[derive(Debug, Default)]
pub struct NoHandshake;

impl Handshake for NoHandshake {
    type Error = Infallible;
    type Transcoder = PlainTranscoder;

    fn new() -> Self {
        NoHandshake
    }

    fn next_stage(self, _input: &[u8]) -> HandshakeResult<Self, Self::Transcoder> {
        HandshakeResult::Complete(PlainTranscoder, vec![])
    }
}

/// Trait allowing transcoding the stream using some form of stream encryption
/// and/or encoding.
pub trait Transcode {
    /// Decodes data received from the remote peer and update the internal state
    /// of the transcoder, either returning response which must be sent to the
    /// remote (see [`DecodedData::Remote`]) or data which should be processed
    /// by the local peer.
    fn decrypt(&mut self, data: &[u8]) -> Vec<u8>;

    /// Encodes data before sending them to the remote peer.
    fn encrypt(&mut self, data: Vec<u8>) -> Vec<u8>;
}

/// Transcoder which does nothing.
#[derive(Debug, Default)]
pub struct PlainTranscoder;

impl Transcode for PlainTranscoder {
    fn decrypt(&mut self, data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }

    fn encrypt(&mut self, data: Vec<u8>) -> Vec<u8> {
        data
    }
}

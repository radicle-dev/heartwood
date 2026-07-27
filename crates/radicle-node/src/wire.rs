//! iroh protocol identifiers and Gossip framing.

use bytes::Buf as _;
use iroh::endpoint::{RecvStream, SendStream};
use radicle_protocol::service::Message;
use radicle_protocol::wire::{self};
use radicle_varint::BufMutExt;

/// ALPN used by long-lived gossip connections.
pub const GOSSIP_ALPN: &[u8] = b"radicle/gossip/1";
/// ALPN used by independent Git fetch connections.
pub const GIT_ALPN: &[u8] = b"radicle/git/1";

/// Gossip codec failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("noq write error: {0}")]
    NoqWrite(#[from] iroh::endpoint::WriteError),
    #[error("noq read exact error: {0}")]
    NoqReadExact(#[from] iroh::endpoint::ReadExactError),
    #[error("gossip message length {0} exceeds the 1 MiB limit")]
    Oversized(usize),
    #[error("invalid gossip message: {0}")]
    Decode(#[from] wire::Error),
    #[error("gossip record contains {0} trailing bytes")]
    Trailing(usize),
}

/// Write one length-delimited Gossip message and return the encoded byte count,
/// including its QUIC-varint framing.
pub async fn write_message(send: &mut SendStream, message: &Message) -> Result<usize, Error> {
    let payload = wire::Encode::encode_to_vec(message);

    let prefix =
        radicle_varint::VarInt::new(payload.len() as u64).expect("1 MiB fits in a QUIC varint");
    let mut buf = Vec::with_capacity(8);
    buf.put_uvar(prefix);

    send.write_all(&buf).await?;
    send.write_all(&payload).await?;
    Ok(buf.len() + payload.len())
}

/// Read one length-delimited Gossip message. Oversized lengths are rejected
/// before allocating their payload. The returned count includes framing.
pub async fn read_message(recv: &mut RecvStream) -> Result<(Message, usize), Error> {
    let (len, prefix_len) = read_varint(recv).await?;
    let len = checked_payload_len(len)?;
    let mut payload = vec![0; len];
    recv.read_exact(&mut payload).await?;
    let mut bytes = payload.as_slice();
    let message = wire::Decode::decode(&mut bytes)?;
    if bytes.has_remaining() {
        return Err(Error::Trailing(bytes.remaining()));
    }
    Ok((message, prefix_len + len))
}

fn checked_payload_len(len: u64) -> Result<usize, Error> {
    let len = usize::try_from(len).map_err(|_| Error::Oversized(usize::MAX))?;
    Ok(len)
}

async fn read_varint(recv: &mut RecvStream) -> Result<(u64, usize), Error> {
    let mut tmp = [0u8; 8];

    // Read the first byte.
    recv.read_exact(&mut tmp[..1]).await?;

    // Check the length of the integer based on the first two bits.
    let len = 1 << usize::from(tmp[0] >> 6);

    // Clear length bits.
    tmp[0] &= 0b0011_1111;

    // Short-circuit if no further read is required.
    if len == 1 {
        return Ok((u64::from(tmp[0]), len));
    }

    // Read the remaining bytes of the integer.
    recv.read_exact(&mut tmp[1..len]).await?;

    let value = match len {
        2 => u64::from(u16::from_be_bytes([tmp[0], tmp[1]])),
        4 => u64::from(u32::from_be_bytes([tmp[0], tmp[1], tmp[2], tmp[3]])),
        8 => u64::from_be_bytes(tmp),
        _ => unreachable!(),
    };

    Ok((value, len))
}

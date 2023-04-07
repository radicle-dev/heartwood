use std::{
    io::{self, Write},
    net, time,
};

use super::channels::Channels;
use super::{pktline, Handle, NodeId, StreamId};

/// Tunnels fetches to a remote peer.
pub struct Tunnel<'a> {
    stream: &'a mut Channels,
    listener: net::TcpListener,
    local_addr: net::SocketAddr,
    channel: StreamId,
    remote: NodeId,
    handle: Handle,
}

impl<'a> Tunnel<'a> {
    pub(super) fn with(
        stream: &'a mut Channels,
        channel: StreamId,
        remote: NodeId,
        handle: Handle,
    ) -> io::Result<Self> {
        let listener = net::TcpListener::bind(net::SocketAddr::from(([0, 0, 0, 0], 0)))?;
        let local_addr = listener.local_addr()?;

        Ok(Self {
            stream,
            listener,
            local_addr,
            channel,
            remote,
            handle,
        })
    }

    pub fn local_addr(&self) -> net::SocketAddr {
        self.local_addr
    }

    /// Run the tunnel until the connection is closed.
    pub fn run(&mut self, timeout: time::Duration) -> io::Result<()> {
        // We now loop, alternating between reading requests from the client, and writing responses
        // back from the daemon.. Requests are delimited with a flush packet (`flush-pkt`).
        let mut buffer = [0; u16::MAX as usize + 1];
        let (mut remote_w, mut remote_r) = self.stream.split();
        let (mut stream, _) = self.listener.accept()?;

        let mut local = pktline::Reader::new(&mut stream);
        let mut remote_r = pktline::Reader::new(&mut remote_r);

        local.stream().set_read_timeout(Some(timeout))?;
        local.stream().set_write_timeout(Some(timeout))?;

        let (_, buf) = local.read_request_pktline()?;
        remote_w.write_all(&buf)?;

        // Nb. Annoyingly, we have to always check if the fetch stream is closed on every
        // iteration, otherwise we may get stuck waiting for data from the remote while
        // we're actually done. After measurement, this checking for EOF only takes
        // between 1µs and 4µs, and is therefore an okay compromise.
        while !local.is_eof()? {
            if self.handle.flush(self.remote, self.channel).is_err() {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            remote_r.pipe(local.stream(), &mut buffer)?;

            if let Err(e) = local.pipe(&mut remote_w, &mut buffer) {
                // This is the expected error when the git fetch closes the connection.
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }
        }
        Ok(())
    }
}

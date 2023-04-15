use std::{
    io::{self, Read},
    net, thread, time,
};

use super::channels::Channels;
use super::{Handle, NodeId, StreamId, Worker};

/// Tunnels fetches to a remote peer.
pub struct Tunnel<'a> {
    channels: &'a mut Channels,
    listener: net::TcpListener,
    local_addr: net::SocketAddr,
    stream: StreamId,
    local: NodeId,
    remote: NodeId,
    handle: Handle,
}

impl<'a> Tunnel<'a> {
    pub(super) fn with(
        channels: &'a mut Channels,
        stream: StreamId,
        local: NodeId,
        remote: NodeId,
        handle: Handle,
    ) -> io::Result<Self> {
        let listener = net::TcpListener::bind(net::SocketAddr::from(([0, 0, 0, 0], 0)))?;
        let local_addr = listener.local_addr()?;

        Ok(Self {
            channels,
            listener,
            local_addr,
            stream,
            local,
            remote,
            handle,
        })
    }

    pub fn local_addr(&self) -> net::SocketAddr {
        self.local_addr
    }

    /// Run the tunnel until the connection is closed.
    pub fn run(&mut self, timeout: time::Duration) -> io::Result<()> {
        let (remote_w, remote_r) = self.channels.split();
        let (local, _) = self.listener.accept()?;
        let (mut local_r, local_w) = (local.try_clone()?, local);

        local_r.set_read_timeout(Some(timeout))?;
        local_w.set_write_timeout(Some(timeout))?;

        let nid = self.remote;
        let stream_id = self.stream;

        thread::scope(|s| {
            let remote_to_local = thread::Builder::new()
                .name(self.local.to_string())
                .spawn_scoped(s, || remote_r.pipe(local_w))?;

            let local_to_remote = thread::Builder::new()
                .name(self.local.to_string())
                .spawn_scoped(s, || {
                    let mut buffer = [0; u16::MAX as usize + 1];

                    loop {
                        match local_r.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => {
                                remote_w.send(buffer[..n].to_vec())?;

                                if let Err(e) = self.handle.flush(nid, stream_id) {
                                    log::error!(
                                        target: "worker", "Worker channel disconnected; aborting"
                                    );
                                    return Err(e);
                                }
                            }
                            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                            Err(e) => return Err(e),
                        }
                    }
                    Worker::eof(nid, stream_id, remote_w, &mut self.handle)
                })?;

            remote_to_local.join().unwrap()?;
            local_to_remote.join().unwrap()?;

            Ok::<(), io::Error>(())
        })
    }
}

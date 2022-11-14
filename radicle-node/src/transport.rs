use std::net;
use std::ops::{Deref, DerefMut};

use nakamoto::LocalTime;
use nakamoto_net as nakamoto;
use nakamoto_net::{Io, Link};

use crate::address;
use crate::collections::HashMap;
use crate::crypto;
use crate::service::routing;
use crate::service::{Command, DisconnectReason, Event, Service};
use crate::storage::WriteStorage;
use crate::wire::transcoder::Transcode;
use crate::wire::Wire;

#[derive(Debug)]
struct Peer {
    _addr: net::SocketAddr,
}

#[derive(Debug)]
pub struct Transport<R, S, W, G, T: Transcode> {
    _peers: HashMap<net::IpAddr, Peer>,
    inner: Wire<R, S, W, G, T>,
}

impl<R, S, W, G, T: Transcode> Transport<R, S, W, G, T> {
    pub fn new(inner: Wire<R, S, W, G, T>) -> Self {
        Self {
            _peers: HashMap::default(),
            inner,
        }
    }
}

impl<R, S, W, G, T> nakamoto::Protocol for Transport<R, S, W, G, T>
where
    R: routing::Store,
    W: WriteStorage + 'static,
    S: address::Store,
    G: crypto::Signer,
    T: Transcode,
{
    type Event = Event;
    type Command = Command;
    type DisconnectReason = DisconnectReason;

    fn initialize(&mut self, time: LocalTime) {
        self.inner.initialize(time)
    }

    fn tick(&mut self, now: nakamoto::LocalTime) {
        self.inner.tick(now)
    }

    fn wake(&mut self) {
        self.inner.wake()
    }

    fn command(&mut self, cmd: Self::Command) {
        self.inner.command(cmd)
    }

    fn attempted(&mut self, addr: &std::net::SocketAddr) {
        self.inner.attempted(addr)
    }

    fn connected(
        &mut self,
        addr: std::net::SocketAddr,
        local_addr: &std::net::SocketAddr,
        link: Link,
    ) {
        self.inner.connected(addr, local_addr, link)
    }

    fn disconnected(
        &mut self,
        addr: &std::net::SocketAddr,
        reason: nakamoto::DisconnectReason<Self::DisconnectReason>,
    ) {
        self.inner.disconnected(addr, reason)
    }

    fn received_bytes(&mut self, addr: &std::net::SocketAddr, bytes: &[u8]) {
        self.inner.received_bytes(addr, bytes)
    }
}

impl<R, S, W, G, T: Transcode> Iterator for Transport<R, S, W, G, T> {
    type Item = Io<Event, DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<R, S, W, G, T: Transcode> Deref for Transport<R, S, W, G, T> {
    type Target = Service<R, S, W, G>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<R, S, W, G, T: Transcode> DerefMut for Transport<R, S, W, G, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

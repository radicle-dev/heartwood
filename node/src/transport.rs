use std::net;
use std::ops::{Deref, DerefMut};

use nakamoto::LocalTime;
use nakamoto_net as nakamoto;
use nakamoto_net::{Io, Link};

use crate::address_book;
use crate::collections::HashMap;
use crate::crypto;
use crate::protocol::{Command, DisconnectReason, Event, Protocol};
use crate::storage::WriteStorage;

#[derive(Debug)]
struct Peer {
    addr: net::SocketAddr,
}

#[derive(Debug)]
pub struct Transport<S, T, G> {
    peers: HashMap<net::IpAddr, Peer>,
    protocol: Protocol<S, T, G>,
}

impl<S, T, G> Transport<S, T, G> {
    pub fn new(protocol: Protocol<S, T, G>) -> Self {
        Self {
            peers: HashMap::default(),
            protocol,
        }
    }
}

impl<'r, S, T, G> nakamoto::Protocol for Transport<S, T, G>
where
    T: WriteStorage<'r> + 'static,
    S: address_book::Store,
    G: crypto::Signer,
{
    type Event = Event;
    type Command = Command;
    type DisconnectReason = DisconnectReason;

    fn initialize(&mut self, time: LocalTime) {
        self.protocol.initialize(time)
    }

    fn tick(&mut self, now: nakamoto::LocalTime) {
        self.protocol.tick(now)
    }

    fn wake(&mut self) {
        self.protocol.wake()
    }

    fn command(&mut self, cmd: Self::Command) {
        self.protocol.command(cmd)
    }

    fn attempted(&mut self, addr: &std::net::SocketAddr) {
        self.protocol.attempted(addr)
    }

    fn connected(
        &mut self,
        addr: std::net::SocketAddr,
        local_addr: &std::net::SocketAddr,
        link: Link,
    ) {
        self.protocol.connected(addr, local_addr, link)
    }

    fn disconnected(
        &mut self,
        addr: &std::net::SocketAddr,
        reason: nakamoto::DisconnectReason<Self::DisconnectReason>,
    ) {
        self.protocol.disconnected(addr, reason)
    }

    fn received_bytes(&mut self, addr: &std::net::SocketAddr, bytes: &[u8]) {
        self.protocol.received_bytes(addr, bytes)
    }
}

impl<S, T, G> Iterator for Transport<S, T, G> {
    type Item = Io<Event, DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        self.protocol.next()
    }
}

impl<S, T, G> Deref for Transport<S, T, G> {
    type Target = Protocol<S, T, G>;

    fn deref(&self) -> &Self::Target {
        &self.protocol
    }
}

impl<S, T, G> DerefMut for Transport<S, T, G> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.protocol
    }
}

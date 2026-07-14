use std::fmt::Display;
use std::io::{Read, Write};
use std::net::SocketAddr;

use mio::net::TcpStream;

pub trait Session: Send + Read + Write {
    type Artifact: Display;

    fn is_established(&self) -> bool {
        self.artifact().is_some()
    }

    fn artifact(&self) -> Option<Self::Artifact>;
}

impl Session for TcpStream {
    type Artifact = SocketAddr;

    fn artifact(&self) -> Option<Self::Artifact> {
        self.peer_addr().ok()
    }
}

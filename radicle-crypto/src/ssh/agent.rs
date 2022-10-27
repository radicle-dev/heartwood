use radicle_ssh::agent::client::AgentClient;
use radicle_ssh::{self as ssh, agent::client::ClientStream};

use crate as crypto;
use crate::ssh::SecretKey;

#[cfg(not(unix))]
use std::net::TcpStream as Stream;
#[cfg(unix)]
use std::os::unix::net::UnixStream as Stream;

pub fn connect() -> Result<AgentClient<Stream>, ssh::agent::client::Error> {
    Stream::connect_env()
}

pub fn register(key: &crypto::SecretKey) -> Result<(), ssh::agent::client::Error> {
    let mut agent = self::connect()?;
    agent.add_identity(&SecretKey::from(*key), &[])?;

    Ok(())
}

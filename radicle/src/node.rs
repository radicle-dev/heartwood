mod features;

use std::fmt;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::identity::Id;

pub use features::Features;

/// Default name for control socket file.
pub const DEFAULT_SOCKET_NAME: &str = "radicle.sock";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to connect to node: {0}")]
    Connect(#[from] io::Error),
}

pub trait Handle {
    /// Fetch a project from the network. Fails if the project isn't tracked.
    fn fetch(&self, id: &Id) -> Result<(), Error>;
    /// Start tracking the given project. Doesn't do anything if the project is already
    /// tracked.
    fn track(&self, id: &Id) -> Result<bool, Error>;
    /// Untrack the given project and delete it from storage.
    fn untrack(&self, id: &Id) -> Result<bool, Error>;
    /// Notify the network that we have new refs.
    fn announce_refs(&self, id: &Id) -> Result<(), Error>;
    /// Ask the node to shutdown.
    fn shutdown(self) -> Result<(), Error>;
}

/// Node control socket.
#[derive(Debug)]
pub struct Connection {
    stream: UnixStream,
}

impl Connection {
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let stream = UnixStream::connect(path).map_err(Error::Connect)?;

        Ok(Self { stream })
    }

    pub fn call<A: fmt::Display>(
        &self,
        cmd: &str,
        arg: &A,
    ) -> Result<impl Iterator<Item = Result<String, io::Error>> + '_, io::Error> {
        writeln!(&self.stream, "{cmd} {arg}")?;

        Ok(BufReader::new(&self.stream).lines())
    }
}

impl Handle for Connection {
    fn fetch(&self, id: &Id) -> Result<(), Error> {
        for line in self.call("fetch", id)? {
            let line = line?;
            log::info!("node: {}", line);
        }
        Ok(())
    }

    fn track(&self, id: &Id) -> Result<bool, Error> {
        for line in self.call("track", id)? {
            let line = line?;
            log::info!("node: {}", line);
        }
        Ok(true)
    }

    fn untrack(&self, id: &Id) -> Result<bool, Error> {
        for line in self.call("untrack", id)? {
            let line = line?;
            log::info!("node: {}", line);
        }
        Ok(true)
    }

    fn announce_refs(&self, id: &Id) -> Result<(), Error> {
        for line in self.call("announce-refs", id)? {
            let line = line?;
            log::info!("node: {}", line);
        }
        Ok(())
    }

    fn shutdown(self) -> Result<(), Error> {
        todo!();
    }
}

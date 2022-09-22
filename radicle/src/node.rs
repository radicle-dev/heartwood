use std::fmt;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::identity::Id;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
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
pub struct Socket {
    stream: UnixStream,
}

impl Socket {
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let stream = UnixStream::connect(path)?;

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

impl Handle for Socket {
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

use std::str::FromStr;
use std::sync::atomic;
use std::{io, net};

use crate::git;

/// Git smart protocol over a TCP stream.
pub struct Smart;

impl git2::transport::SmartSubtransport for Smart {
    fn action(
        &self,
        url: &str,
        action: git2::transport::Service,
    ) -> Result<Box<dyn git2::transport::SmartSubtransportStream>, git2::Error> {
        let url = git::Url::from_bytes(url.as_bytes())
            .map_err(|e| git2::Error::from_str(e.to_string().as_str()))?;

        let addr = if let (Some(host), Some(port)) = (url.host, url.port) {
            // TODO: Support hostnames.
            net::SocketAddr::new(
                net::IpAddr::from_str(&host)
                    .map_err(|e| git2::Error::from_str(e.to_string().as_str()))?,
                port,
            )
        } else {
            return Err(git2::Error::from_str("Git URL must have a host and port"));
        };

        let stream = std::net::TcpStream::connect(addr)
            .map_err(|e| git2::Error::from_str(e.to_string().as_str()))?;

        match action {
            git2::transport::Service::UploadPackLs => {}
            git2::transport::Service::UploadPack => {}
            git2::transport::Service::ReceivePack => {
                return Err(git2::Error::from_str(
                    "git-receive-pack is not supported with the custom transport",
                ));
            }
            git2::transport::Service::ReceivePackLs => {
                return Err(git2::Error::from_str(
                    "git-receive-pack is not supported with the custom transport",
                ));
            }
        }
        Ok(Box::new(Stream { stream }))
    }

    fn close(&self) -> Result<(), git2::Error> {
        Ok(())
    }
}

struct Stream {
    stream: std::net::TcpStream,
}

impl io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

impl io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

/// Register the "smart" transport with `git`.
pub fn register(prefix: &str) -> Result<(), git2::Error> {
    static REGISTERED: atomic::AtomicBool = atomic::AtomicBool::new(false);

    if !REGISTERED.swap(true, atomic::Ordering::SeqCst) {
        unsafe {
            git2::transport::register(prefix, move |remote| {
                git2::transport::Transport::smart(remote, false, Smart)
            })
        }
    } else {
        Err(git2::Error::from_str(
            "custom git transport is already registered",
        ))
    }
}

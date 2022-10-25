pub mod local;
pub mod remote;

use std::{io, process};

/// A wrapper around a child process' stdin and stdout,
/// making it [`io::Read`] and [`io::Write`].
///
/// Used for some of the git transports.
pub(crate) struct ChildStream {
    pub stdin: process::ChildStdin,
    pub stdout: process::ChildStdout,
}

impl io::Read for ChildStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stdout.read(buf)
    }
}

impl io::Write for ChildStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdin.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdin.flush()
    }
}

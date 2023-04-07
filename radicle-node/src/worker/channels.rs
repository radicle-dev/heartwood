use std::io::{Read, Write};
use std::{fmt, io};

use crossbeam_channel as chan;

/// Data that can be sent and received on worker channels.
pub enum ChannelEvent<T = Vec<u8>> {
    /// Git protocol data.
    Data(T),
    /// A request to close the channel.
    Close,
    /// A signal that the git protocol has ended, eg. when the remote fetch closes the
    /// connection.
    Eof,
}

impl<T> fmt::Debug for ChannelEvent<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(_) => write!(f, "ChannelEvent::Data(..)"),
            Self::Close => write!(f, "ChannelEvent::Close"),
            Self::Eof => write!(f, "ChannelEvent::Eof"),
        }
    }
}

/// Worker channels for communicating through the git stream with the remote.
pub struct Channels<T = Vec<u8>> {
    pub sender: ChannelWriter<T>,
    pub receiver: ChannelReader<T>,
}

impl Write for Channels {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.sender.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sender.flush()
    }
}

impl Read for Channels {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.receiver.read(buf)
    }
}

impl<T> Channels<T> {
    pub fn new(
        sender: chan::Sender<ChannelEvent<T>>,
        receiver: chan::Receiver<ChannelEvent<T>>,
    ) -> Self {
        Channels {
            sender: ChannelWriter(sender),
            receiver: ChannelReader {
                receiver,
                buffer: io::Cursor::new(Vec::new()),
            },
        }
    }

    pub fn split(&mut self) -> (&mut ChannelWriter<T>, &mut ChannelReader<T>) {
        (&mut self.sender, &mut self.receiver)
    }
}

/// Wraps a [`chan::Receiver`] and provides it with [`io::Read`].
#[derive(Clone)]
pub struct ChannelReader<T = Vec<u8>> {
    buffer: io::Cursor<Vec<u8>>,
    receiver: chan::Receiver<ChannelEvent<T>>,
}

impl Read for ChannelReader<Vec<u8>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.buffer.read(buf)?;
        if read == 0 {
            let event = self.receiver.recv().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "error reading from stream: channel is disconnected",
                )
            })?;

            match event {
                ChannelEvent::Data(data) => {
                    self.buffer = io::Cursor::new(data);
                    self.buffer.read(buf)
                }
                ChannelEvent::Eof => Err(io::ErrorKind::UnexpectedEof.into()),
                ChannelEvent::Close => Err(io::ErrorKind::ConnectionReset.into()),
            }
        } else {
            Ok(read)
        }
    }
}

/// Wraps a [`chan::Sender`] and provides it with [`io::Write`].
#[derive(Clone)]
pub struct ChannelWriter<T = Vec<u8>>(chan::Sender<ChannelEvent<T>>);

impl ChannelWriter {
    /// Since the git protocol is tunneled over an existing connection, we can't signal the end of
    /// the protocol via the usual means, which is to close the connection. Git also doesn't have
    /// any special message we can send to signal the end of the protocol.
    ///
    /// Hence, we there's no other way for the server to know that we're done sending requests
    /// than to send a special message outside the git protocol. This message can then be processed
    /// by the remote worker to end the protocol. We use the special "eof" control message for this.
    pub fn eof(&self) -> Result<(), chan::SendError<ChannelEvent>> {
        self.0.send(ChannelEvent::Eof)
    }
}

impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let data = buf.to_vec();
        self.0.send(ChannelEvent::Data(data)).map_err(|m| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!(
                    "error writing to stream: channel is disconnected: dropped {:?}",
                    m.into_inner()
                ),
            )
        })?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

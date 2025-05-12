use std::convert::Infallible;
use std::io::{Read, Write};
use std::ops::Deref;
use std::{fmt, io, time};

use crossbeam_channel as chan;
use radicle::node::config::FetchPackSizeLimit;
use radicle::node::NodeId;

use crate::runtime::Handle;
use crate::wire::{StreamError, StreamId};

/// Maximum size of channel used to communicate with a worker.
/// Note that as long as we're using [`std::io::copy`] to copy data from the
/// upload-pack's stdout, the data chunks are of a maximum size of 8192 bytes.
pub const MAX_WORKER_CHANNEL_SIZE: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct ChannelsConfig {
    timeout: time::Duration,
    reader_limit: FetchPackSizeLimit,
}

impl ChannelsConfig {
    pub fn new(timeout: time::Duration) -> Self {
        Self {
            timeout,
            reader_limit: FetchPackSizeLimit::default(),
        }
    }

    pub fn with_timeout(self, timeout: time::Duration) -> Self {
        Self { timeout, ..self }
    }

    pub fn with_reader_limit(self, reader_limit: FetchPackSizeLimit) -> Self {
        Self {
            reader_limit,
            ..self
        }
    }
}

/// A reader and writer pair that can be used in the fetch protocol.
///
/// It implements [`radicle::fetch::transport::ConnectionStream`] to
/// provide its underlying channels for reading and writing.
pub struct ChannelsFlush {
    receiver: ChannelReader,
    sender: ChannelFlushWriter,
}

impl ChannelsFlush {
    pub fn new(handle: Handle, channels: Channels, remote: NodeId, stream: StreamId) -> Self {
        Self {
            receiver: channels.receiver,
            sender: ChannelFlushWriter {
                writer: channels.sender,
                stream,
                handle,
                remote,
            },
        }
    }

    pub fn split(&mut self) -> (&mut ChannelReader, &mut ChannelFlushWriter) {
        (&mut self.receiver, &mut self.sender)
    }

    pub fn timeout(&self) -> time::Duration {
        self.sender.writer.timeout.max(self.receiver.timeout)
    }
}

impl radicle_fetch::transport::ConnectionStream for ChannelsFlush {
    type Read = ChannelReader;
    type Write = ChannelFlushWriter;
    type Error = Infallible;

    fn open(&mut self) -> Result<(&mut Self::Read, &mut Self::Write), Self::Error> {
        Ok((&mut self.receiver, &mut self.sender))
    }
}

/// Data that can be sent and received on worker channels.
pub enum ChannelEvent<T = Vec<u8>, E = StreamError> {
    /// Git protocol data.
    Data(T),
    /// A signal that the git protocol has ended, eg. when the remote fetch closes the
    /// connection.
    Close,
    Error(E),
}

impl<T> From<T> for ChannelEvent<T> {
    fn from(value: T) -> Self {
        Self::Data(value)
    }
}

impl<T> fmt::Debug for ChannelEvent<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(_) => write!(f, "ChannelEvent::Data(..)"),
            Self::Close => write!(f, "ChannelEvent::Close"),
            Self::Error(err) => write!(f, "ChannelEvent::Error({})", err),
        }
    }
}

/// Worker channels for communicating through the git stream with the remote.
pub struct Channels<T = Vec<u8>> {
    sender: ChannelWriter<T>,
    receiver: ChannelReader<T>,
}

impl<T: AsRef<[u8]>> Channels<T> {
    pub fn new(
        sender: chan::Sender<ChannelEvent<T>>,
        receiver: chan::Receiver<ChannelEvent<T>>,
        config: ChannelsConfig,
    ) -> Self {
        let sender = ChannelWriter {
            sender,
            timeout: config.timeout,
        };
        let receiver = ChannelReader::new(receiver, config.timeout, config.reader_limit);

        Self { sender, receiver }
    }

    pub fn pair(config: ChannelsConfig) -> io::Result<(Channels<T>, Channels<T>)> {
        let (l_send, r_recv) = chan::bounded::<ChannelEvent<T>>(MAX_WORKER_CHANNEL_SIZE);
        let (r_send, l_recv) = chan::bounded::<ChannelEvent<T>>(MAX_WORKER_CHANNEL_SIZE);

        let l = Channels::new(l_send, l_recv, config);
        let r = Channels::new(r_send, r_recv, config);

        Ok((l, r))
    }

    pub fn try_iter(&self) -> impl Iterator<Item = ChannelEvent<T>> + '_ {
        self.receiver.try_iter()
    }

    pub fn send(&self, event: ChannelEvent<T>) -> io::Result<()> {
        self.sender.send(event)
    }

    pub fn close(self) -> Result<(), chan::SendError<ChannelEvent<T>>> {
        self.sender.close()
    }

    pub fn error(&mut self, error: StreamError) -> Result<(), chan::SendError<ChannelEvent<T>>> {
        self.sender.error(error)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReadLimiter {
    limit: FetchPackSizeLimit,
    total_read: usize,
}

impl ReadLimiter {
    pub fn new(limit: FetchPackSizeLimit) -> Self {
        Self {
            limit,
            total_read: 0,
        }
    }

    pub fn read(&mut self, bytes: usize) -> io::Result<()> {
        self.total_read = self.total_read.saturating_add(bytes);
        log::trace!(target: "worker", "limit {}, total bytes read: {}", self.limit, self.total_read);
        if self.limit.exceeded_by(self.total_read) {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "sender has exceeded number of allowed bytes, aborting read",
            ))
        } else {
            Ok(())
        }
    }
}

/// Wraps a [`chan::Receiver`] and provides it with [`io::Read`].
#[derive(Clone)]
pub struct ChannelReader<T = Vec<u8>> {
    buffer: io::Cursor<Vec<u8>>,
    receiver: chan::Receiver<ChannelEvent<T>>,
    timeout: time::Duration,
    limiter: ReadLimiter,
}

impl<T> Deref for ChannelReader<T> {
    type Target = chan::Receiver<ChannelEvent<T>>;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<T: AsRef<[u8]>> ChannelReader<T> {
    pub fn new(
        receiver: chan::Receiver<ChannelEvent<T>>,
        timeout: time::Duration,
        limit: FetchPackSizeLimit,
    ) -> Self {
        Self {
            buffer: io::Cursor::new(Vec::new()),
            receiver,
            timeout,
            limiter: ReadLimiter::new(limit),
        }
    }
}

impl Read for ChannelReader<Vec<u8>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.buffer.read(buf)?;
        self.limiter.read(read)?;
        if read > 0 {
            return Ok(read);
        }

        match self.receiver.recv_timeout(self.timeout) {
            Ok(ChannelEvent::Data(data)) => {
                self.buffer = io::Cursor::new(data);
                self.buffer.read(buf)
            }
            Ok(ChannelEvent::Close) => Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(ChannelEvent::Error(StreamError::Io(kind))) => Err(io::Error::new(
                kind,
                format!(
                    "error reading from stream: other side reported i/o error: {}",
                    kind
                ),
            )),
            Ok(ChannelEvent::Error(err)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "error reading from stream: other side reported error: {}",
                    err
                ),
            )),

            Err(chan::RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "error reading from stream: channel timed out",
            )),
            Err(chan::RecvTimeoutError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "error reading from stream: channel is disconnected",
            )),
        }
    }
}

/// Wraps a [`chan::Sender`] and provides it with [`io::Write`].
#[derive(Clone)]
struct ChannelWriter<T = Vec<u8>, E = StreamError> {
    sender: chan::Sender<ChannelEvent<T, E>>,
    timeout: time::Duration,
}

/// Wraps a [`ChannelWriter`] alongside the associated [`Handle`] and [`NodeId`].
///
/// This allows the channel to [`Write::flush`] when calling
/// [`Write::write`], which is necessary to signal to the
/// controller to send the wire data.
pub struct ChannelFlushWriter<T = Vec<u8>> {
    writer: ChannelWriter<T>,
    handle: Handle,
    stream: StreamId,
    remote: NodeId,
}

impl radicle_fetch::transport::Close for ChannelFlushWriter<Vec<u8>> {
    type Error = io::Error;

    fn close(&mut self) -> io::Result<()> {
        self.writer.send(ChannelEvent::Close)?;
        self.flush()
    }
}

impl Write for ChannelFlushWriter<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = buf.len();
        self.writer.send(buf.to_vec())?;
        self.flush()?;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.handle.flush(self.remote, self.stream)
    }
}

impl<T: AsRef<[u8]>> ChannelWriter<T> {
    pub fn send(&self, event: impl Into<ChannelEvent<T>>) -> io::Result<()> {
        match self.sender.send_timeout(event.into(), self.timeout) {
            Ok(()) => Ok(()),
            Err(chan::SendTimeoutError::Timeout(_)) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "error writing to stream: channel timed out",
            )),
            Err(chan::SendTimeoutError::Disconnected(_)) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "error writing to stream: channel is disconnected",
            )),
        }
    }

    /// Permanently close this stream.
    pub fn close(self) -> Result<(), chan::SendError<ChannelEvent<T>>> {
        self.sender.send(ChannelEvent::Close)
    }

    /// Mark this stream as errored.
    pub fn error(&mut self, error: StreamError) -> Result<(), chan::SendError<ChannelEvent<T>>> {
        self.sender.send(ChannelEvent::Error(error))
    }
}

mod controller;
mod listener;
mod session;
mod timer;
mod token;
mod transport;

use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::io::ErrorKind;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{io, thread};

use crossbeam_channel::{unbounded, Receiver, TryRecvError};
use localtime::LocalTime;
use mio::event::{Event, Source};
use mio::{Events, Interest, Poll, Waker};
use thiserror::Error;

use timer::Timer;
use token::WAKER;

use crate::wire;

pub(crate) use self::controller::{ControlMessage, Controller};
pub(crate) use listener::Listener;
pub use session::{NoiseSession, ProtocolArtifact, Socks5Session};
pub(crate) use token::{Token, Tokens};
pub(crate) use transport::{SessionEvent, Transport};

const SECONDS_IN_AN_HOUR: u64 = 60 * 60;

/// Maximum amount of time to wait for I/O.
const WAIT_TIMEOUT: Duration = Duration::from_secs(SECONDS_IN_AN_HOUR);

/// A resource which can be managed by the reactor.
pub trait EventHandler {
    /// The type of reactions which this resource may generate upon receiving
    /// I/O from the reactor via [`EventHandler::handle`]. These events are
    /// passed to the reactor [`crate::reactor::ReactionHandler`].
    type Reaction;

    /// Method informing the reactor which types of events this resource is subscribed for.
    fn interests(&self) -> Option<Interest>;

    /// Method called by the reactor when an I/O readiness event
    /// is received for this resource.
    fn handle(&mut self, event: &Event) -> Vec<Self::Reaction>;
}

/// The trait guarantees that the data are either written in full or, in case
/// of an error, none of the data is written. Types implementing the trait must
/// also guarantee that multiple attempts to write do not result in
/// data to be written out of the initial ordering.
pub trait WriteAtomic: std::io::Write {
    /// Atomic non-blocking I/O write operation, which must either write the whole buffer to a
    /// resource without blocking or fail.
    ///
    /// # Panics
    ///
    /// If [`WriteAtomic::write_or_buf`] returns an [`std::io::Error`] of kind
    /// [`ErrorKind::Interrupted`], [`ErrorKind::WouldBlock`], [`ErrorKind::WriteZero`].
    /// In this case, [`WriteAtomic::write_or_buf`] is expected to buffer.
    fn write_atomic(&mut self, buf: &[u8]) -> io::Result<()> {
        use ErrorKind::*;

        if !self.is_ready_to_write() {
            panic!("WriteAtomic::write_atomic was called when the resource is not ready to write");
        }

        let result = self.write_or_buf(buf);

        debug_assert!(
            !matches!(
                result.as_ref().err().map(|err| err.kind()),
                Some(Interrupted | WouldBlock | WriteZero)
            ),
            "WriteAtomic::write_or_buf must handle erros of kind {Interrupted:?}, {WouldBlock:?}, {WriteZero:?} by buffering",
        );

        result
    }

    /// Checks whether resource can be written to without blocking.
    fn is_ready_to_write(&self) -> bool;

    /// Writes to the resource in a non-blocking way, buffering the data if necessary,
    /// or failing with a system-level error.
    ///
    /// This method shouldn't be called directly; call [`WriteAtomic::write_atomic`] instead.
    ///
    /// The method must handle [`std::io::Error`] of kind
    /// [`ErrorKind::Interrupted`], [`ErrorKind::WouldBlock`], [`ErrorKind::WriteZero`].
    /// and buffer the data in such cases.
    fn write_or_buf(&mut self, buf: &[u8]) -> io::Result<()>;
}

/// Reactor errors
#[derive(Error)]
pub enum Error<L: EventHandler, T: EventHandler> {
    #[error("listener {0:?} got disconnected during poll operation")]
    ListenerDisconnect(Token, L),

    #[error("transport {0:?} got disconnected during poll operation")]
    TransportDisconnect(Token, T),

    #[error("registration of a resource has failed: {0}")]
    Poll(io::Error),

    #[error("registration of a resource has failed: {0}")]
    Registration(io::Error),
}

impl<L: EventHandler, T: EventHandler> Debug for Error<L, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

/// Actions which can be provided to the [`Reactor`] by the [`ReactionHandler`].
///
/// Reactor reads actions on each event loop using [`ReactionHandler`] iterator interface.
pub enum Action<L, T> {
    /// Register a new listener resource for the reactor poll.
    ///
    /// Reactor can't instantiate the resource, like bind a network listener.
    /// Reactor only can register already active resource for polling in the event loop.
    RegisterListener(Token, L),

    /// Register a new transport resource for the reactor poll.
    ///
    /// Reactor can't instantiate the resource, like open a file or establish network connection.
    /// Reactor only can register already active resource for polling in the event loop.
    RegisterTransport(Token, T),

    /// Unregister listener resource from the reactor poll and handover it to the [`ReactionHandler`] via
    /// [`ReactionHandler::handover_listener`].
    ///
    /// When the resource is unregistered no action is performed, i.e. the file descriptor is not
    /// closed, listener is not unbound, connections are not closed etc. All these actions must be
    /// handled by the handler upon the handover event.
    #[allow(dead_code)] // For future use
    UnregisterListener(Token),

    /// Unregister transport resource from the reactor poll and handover it to the [`ReactionHandler`] via
    /// [`ReactionHandler::handover_transport`].
    ///
    /// When the resource is unregistered no action is performed, i.e. the file descriptor is not
    /// closed, listener is not unbound, connections are not closed etc. All these actions must be
    /// handled by the handler upon the handover event.
    UnregisterTransport(Token),

    /// Write the data to one of the transport resources using [`io::Write`].
    Send(Token, Vec<u8>),

    /// Set a new timer for a given duration from this moment.
    ///
    /// When the timer elapses, the reactor will timeout from poll and call
    /// [`ReactionHandler::timer_reacted`].
    SetTimer(Duration),
}

impl<L: EventHandler, T: EventHandler> Display for Action<L, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::RegisterListener(token, _listener) => f
                .debug_struct("RegisterListener")
                .field("token", token)
                .field("listener", &"<omitted>")
                .finish(),
            Action::RegisterTransport(token, _transport) => f
                .debug_struct("RegisterTransport")
                .field("token", token)
                .field("transport", &"<omitted>")
                .finish(),
            Action::UnregisterListener(token) => f
                .debug_struct("UnregisterListener")
                .field("token", token)
                .finish(),
            Action::UnregisterTransport(token) => f
                .debug_struct("UnregisterTransport")
                .field("token", token)
                .finish(),
            Action::Send(token, _data) => f
                .debug_struct("Send")
                .field("token", token)
                .field("data", &"<omitted>")
                .finish(),
            Action::SetTimer(duration) => f
                .debug_struct("SetTimer")
                .field("duration", duration)
                .finish(),
        }
    }
}

/// A service which handles reactions to the events generated in the [`Reactor`].
pub trait ReactionHandler: Send + Iterator<Item = Action<Self::Listener, Self::Transport>> {
    /// Type for a listener resource.
    ///
    /// Listener resources are resources which may spawn more resources and can't be written to. A
    /// typical example of a listener resource is a [`std::net::TcpListener`], however this may also
    /// be a special form of a peripheral device or something else.
    type Listener: EventHandler + Source + Send + Debug;

    /// Type for a transport resource.
    ///
    /// Transport is a "full" resource which can be read from - and written to. Usual files, network
    /// connections, database connections etc are all fall into this category.
    type Transport: EventHandler + Source + Send + Debug + WriteAtomic;

    /// Method called by the reactor on the start of each event loop once the poll has returned.
    fn tick(&mut self, time: localtime::LocalTime);

    /// Method called by the reactor when a previously set timeout is fired.
    ///
    /// Related: [`Action::SetTimer`].
    fn timer_reacted(&mut self);

    /// Method called by the reactor upon a reaction to an I/O event on a listener resource.
    ///
    /// Since listener doesn't support writing, it can be only a read event (indicating that a new
    /// resource can be spawned from the listener).
    fn listener_reacted(
        &mut self,
        token: Token,
        reaction: <Self::Listener as EventHandler>::Reaction,
        time: localtime::LocalTime,
    );

    /// Method called by the reactor upon a reaction to an I/O event on a transport resource.
    fn transport_reacted(
        &mut self,
        token: Token,
        reaction: <Self::Transport as EventHandler>::Reaction,
        time: localtime::LocalTime,
    );

    /// Method called by the reactor when a given resource was successfully registered
    /// for given token.
    ///
    /// The token will be used later in [`ReactionHandler::listener_reacted`]
    /// and [`ReactionHandler::handover_listener`] calls to the handler.
    fn listener_registered(&mut self, token: Token, listener: &Self::Listener);

    /// Method called by the reactor when a given resource was successfully registered
    /// for given token.
    ///
    /// The token will be used later in [`ReactionHandler::transport_reacted`],
    /// [`ReactionHandler::handover_transport`] calls to the handler.
    fn transport_registered(&mut self, token: Token, transport: &Self::Transport);

    /// Method called by the reactor when a command is received for the
    /// [`ReactionHandler`].
    ///
    /// The commands are sent via `Controller` from outside of the reactor, including other
    /// threads.
    fn handle_command(&mut self, cmd: wire::Control);

    /// Method called by the reactor on any kind of error during the event loop, including errors of
    /// the poll syscall or I/O errors returned as a part of the poll result events.
    ///
    /// See [`enum@Error`] for the details on errors which may happen.
    fn handle_error(&mut self, err: Error<Self::Listener, Self::Transport>);

    /// Method called by the reactor upon receiving [`Action::UnregisterListener`].
    ///
    /// Passes the listener resource to the [`ReactionHandler`] when it is already not a part of the reactor
    /// poll. From this point of time it is safe to send the resource to other threads (like
    /// workers) or close the resource.
    fn handover_listener(&mut self, token: Token, listener: Self::Listener);

    /// Method called by the reactor upon receiving [`Action::UnregisterTransport`].
    ///
    /// Passes the transport resource to the [`ReactionHandler`] when it is already not a part of the
    /// reactor poll. From this point of time it is safe to send the resource to other threads
    /// (like workers) or close the resource.
    fn handover_transport(&mut self, token: Token, transport: Self::Transport);
}

/// High-level reactor API wrapping reactor [`Runtime`] into a thread and providing basic thread
/// management for it.
///
/// Apps running the [`Reactor`] can interface it and a [`ReactionHandler`] via use of the `Controller`
/// API.
pub struct Reactor {
    thread: JoinHandle<()>,
    controller: Controller,
}

impl Reactor {
    /// Creates new reactor and a service exposing the [`ReactionHandler`] to
    /// the reactor.
    ///
    /// The service is sent to the newly created reactor thread which runs the
    /// reactor [`Runtime`].
    pub fn new<H>(service: H, thread_name: String) -> Result<Self, io::Error>
    where
        H: 'static + ReactionHandler,
    {
        let builder = thread::Builder::new().name(thread_name);
        let (sender, receiver) = unbounded();
        let poll = Poll::new()?;
        let controller = Controller::new(sender, Arc::new(Waker::new(poll.registry(), WAKER)?));

        log::debug!(target: "reactor-controller", "Initializing reactor thread...");
        let thread = builder.spawn(move || {
            let runtime = Runtime {
                service,
                poll,
                receiver,
                listeners: HashMap::new(),
                transports: HashMap::new(),
                timeouts: Timer::new(),
            };

            log::info!(target: "reactor", "Entering reactor event loop");

            runtime.run();
        })?;

        // Waking up to consume actions which were provided by the service on launch
        controller.wake()?;

        Ok(Self { thread, controller })
    }

    /// Provides a `Controller` that can be used to send events to
    /// [`ReactionHandler`] via self.
    pub fn controller(&self) -> Controller {
        self.controller.clone()
    }

    /// Joins the reactor thread.
    pub fn join(self) -> thread::Result<()> {
        self.thread.join()
    }
}

/// Internal [`Reactor`] runtime which is run in a dedicated thread.
///
/// This runtime structure *does not* spawn a thread and is *blocking*.
/// It implements the actual reactor event loop.
pub struct Runtime<H: ReactionHandler> {
    service: H,
    poll: Poll,
    receiver: Receiver<ControlMessage>,
    listeners: HashMap<Token, H::Listener>,
    transports: HashMap<Token, H::Transport>,
    timeouts: Timer,
}

impl<H: ReactionHandler> Runtime<H> {
    fn register_interests(&mut self) -> io::Result<()> {
        let registry = self.poll.registry();
        for (id, res) in self.listeners.iter_mut() {
            match res.interests() {
                None => registry.deregister(res)?,
                Some(interests) => registry.reregister(res, *id, interests)?,
            };
        }
        for (id, res) in self.transports.iter_mut() {
            match res.interests() {
                None => registry.deregister(res)?,
                Some(interests) => registry.reregister(res, *id, interests)?,
            };
        }
        Ok(())
    }

    fn run(mut self) {
        loop {
            let before_poll = LocalTime::now();
            let timeout = self
                .timeouts
                .next_expiring_from(before_poll)
                .unwrap_or(WAIT_TIMEOUT);

            self.register_interests()
                .expect("registering interests must work to ensure correct operation");

            log::trace!(target: "reactor", "Polling with timeout {timeout:?}");

            let mut events = Events::with_capacity(1024);

            // Blocking
            let res = self.poll.poll(&mut events, Some(timeout));

            let now = LocalTime::now();
            self.service.tick(now);

            // The way this is currently used basically ignores which keys have
            // timed out. So as long as *something* timed out, we wake the service.
            let timers_fired = self.timeouts.remove_expired_by(now);
            if timers_fired > 0 {
                log::trace!(target: "reactor", "Timer has fired");
                self.service.timer_reacted();
            }

            if let Err(err) = res {
                log::error!(target: "reactor", "Error during polling: {err}");
                self.service.handle_error(Error::Poll(err));
            }

            let awoken = self.handle_events(now, events);

            // Process the commands only if we awoken by the waker.
            if awoken {
                loop {
                    match self.receiver.try_recv() {
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            panic!("control channel disconnected unexpectedly")
                        }
                        Ok(ControlMessage::Shutdown) => return self.handle_shutdown(),
                        Ok(ControlMessage::Command(cmd)) => self.service.handle_command(*cmd),
                    }
                }
            }

            self.handle_actions(now);
        }
    }

    /// # Returns
    ///
    /// Whether one of the events was originated from the waker.
    fn handle_events(&mut self, time: LocalTime, events: Events) -> bool {
        log::trace!(target: "reactor", "Handling events");
        let mut awoken = false;
        let mut deregistered = Vec::new();

        for event in events.into_iter() {
            let token = event.token();

            if token == WAKER {
                log::trace!(target: "reactor", "Awoken by the controller");
                awoken = true;
            } else if self.listeners.contains_key(&token) {
                log::trace!(target: "reactor", token=token.0; "Event from listener with token {}: {:?}", token.0, event);
                if !event.is_error() {
                    let listener = self
                        .listeners
                        .get_mut(&token)
                        .expect("resource disappeared");
                    listener
                        .handle(event)
                        .into_iter()
                        .for_each(|service_event| {
                            self.service.listener_reacted(token, service_event, time);
                        });
                } else {
                    let listener = self.deregister_listener(token).unwrap_or_else(|| {
                        panic!("listener with token {} has disappeared", token.0)
                    });
                    self.service
                        .handle_error(Error::ListenerDisconnect(token, listener));
                    deregistered.push(token);
                }
            } else if self.transports.contains_key(&token) {
                log::trace!(target: "reactor", token=token.0; "Event from transport with token {}: {:?}", token.0, event);
                if !event.is_error() {
                    let transport = self
                        .transports
                        .get_mut(&token)
                        .expect("resource disappeared");
                    transport
                        .handle(event)
                        .into_iter()
                        .for_each(|service_event| {
                            self.service.transport_reacted(token, service_event, time);
                        });
                } else {
                    let transport = self.deregister_transport(token).unwrap_or_else(|| {
                        panic!("transport with token {} has disappeared", token.0)
                    });
                    self.service
                        .handle_error(Error::TransportDisconnect(token, transport));
                    deregistered.push(token);
                }
            } else if !deregistered.contains(&token) {
                log::warn!(target: "reactor", token=token.0; "Event from unknown token {}: {:?}", token.0, event);
            }
        }

        awoken
    }

    fn handle_actions(&mut self, time: LocalTime) {
        while let Some(action) = self.service.next() {
            log::trace!(target: "reactor", "Handling action {action} from the service");

            // Deadlock may happen here if the service will generate events over and over
            // in the handle_* calls we may never get out of this loop
            if let Err(err) = self.handle_action(action, time) {
                log::error!(target: "reactor", "Error: {err}");
                self.service.handle_error(err);
            }
        }
    }

    fn handle_action(
        &mut self,
        action: Action<H::Listener, H::Transport>,
        time: LocalTime,
    ) -> Result<(), Error<H::Listener, H::Transport>> {
        match action {
            Action::RegisterListener(token, mut listener) => {
                log::trace!(target: "reactor", token=token.0; "Registering listener {:?} with token {}", listener, token.0);

                self.poll
                    .registry()
                    .register(&mut listener, token, Interest::READABLE)
                    .map_err(Error::Registration)?;
                self.listeners.insert(token, listener);
                self.service
                    .listener_registered(token, &self.listeners[&token]);
            }
            Action::RegisterTransport(token, mut transport) => {
                log::debug!(target: "reactor", token=token.0; "Registering transport");

                self.poll
                    .registry()
                    .register(&mut transport, token, Interest::READABLE)
                    .map_err(Error::Registration)?;
                self.transports.insert(token, transport);
                self.service
                    .transport_registered(token, &self.transports[&token]);
            }
            Action::UnregisterListener(token) => {
                let Some(listener) = self.deregister_listener(token) else {
                    return Ok(());
                };

                log::debug!(target: "reactor", token=token.0; "Handing over listener {listener:?} with token {}", token.0);
                self.service.handover_listener(token, listener);
            }
            Action::UnregisterTransport(token) => {
                let Some(transport) = self.deregister_transport(token) else {
                    return Ok(());
                };

                log::debug!(target: "reactor", token=token.0; "Handing over transport {transport:?} with token {}", token.0);
                self.service.handover_transport(token, transport);
            }
            Action::Send(token, data) => {
                log::trace!(target: "reactor", token=token.0; "Sending {} bytes to {token:?}", data.len());

                if let Some(transport) = self.transports.get_mut(&token) {
                    if let Err(e) = transport.write_atomic(&data) {
                        log::error!(target: "reactor", "Fatal error writing to transport {token:?}, disconnecting. Error details: {e:?}");
                        if let Some(transport) = self.deregister_transport(token) {
                            return Err(Error::TransportDisconnect(token, transport));
                        }
                    }
                } else {
                    log::error!(target: "reactor", token=token.0; "No transport with token {token:?} is known!");
                }
            }
            Action::SetTimer(duration) => {
                log::trace!(target: "reactor", "Adding timer {duration:?} from now");

                self.timeouts.set_timeout(duration, time);
            }
        }
        Ok(())
    }

    fn handle_shutdown(self) {
        log::info!(target: "reactor", "Shutdown");
    }

    fn deregister_listener(&mut self, token: Token) -> Option<H::Listener> {
        let Some(mut source) = self.listeners.remove(&token) else {
            log::warn!(target: "reactor", token=token.0; "Deregistering non-registered listener with token {}", token.0);
            return None;
        };

        if let Err(err) = self.poll.registry().deregister(&mut source) {
            log::warn!(target: "reactor", token=token.0; "Failed to deregister listener with token {} from mio: {err}", token.0);
        }

        Some(source)
    }

    fn deregister_transport(&mut self, token: Token) -> Option<H::Transport> {
        let Some(mut source) = self.transports.remove(&token) else {
            log::warn!(target: "reactor", token=token.0; "Deregistering non-registered transport with token {}", token.0);
            return None;
        };

        if let Err(err) = self.poll.registry().deregister(&mut source) {
            log::warn!(target: "reactor", token=token.0; "Failed to deregister transport with token {} from mio: {err}", token.0);
        }

        Some(source)
    }
}

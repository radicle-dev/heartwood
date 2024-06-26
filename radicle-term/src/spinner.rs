use std::io::{IsTerminal, Write};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use std::{fmt, io, thread, time};

use crossbeam_channel as chan;

use radicle_signals as signals;
use signals::Signal;

use crate::io::{ERROR_PREFIX, WARNING_PREFIX};
use crate::Paint;

/// How much time to wait between spinner animation updates.
pub const DEFAULT_TICK: time::Duration = time::Duration::from_millis(99);
/// The spinner animation strings.
pub const DEFAULT_STYLE: [Paint<&'static str>; 4] = [
    Paint::magenta("◢"),
    Paint::cyan("◣"),
    Paint::magenta("◤"),
    Paint::blue("◥"),
];

struct Progress {
    state: State,
    message: Paint<String>,
}

impl Progress {
    fn new(message: Paint<String>) -> Self {
        Self {
            state: State::Running { cursor: 0 },
            message,
        }
    }
}

enum State {
    Running { cursor: usize },
    Canceled,
    Done,
    Warn,
    Error,
}

/// A progress spinner.
pub struct Spinner {
    progress: Arc<Mutex<Progress>>,
    handle: ManuallyDrop<thread::JoinHandle<()>>,
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Ok(mut progress) = self.progress.lock() {
            if let State::Running { .. } = progress.state {
                progress.state = State::Canceled;
            }
        }
        unsafe { ManuallyDrop::take(&mut self.handle) }
            .join()
            .unwrap();
    }
}

impl Spinner {
    /// Mark the spinner as successfully completed.
    pub fn finish(self) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.state = State::Done;
        }
    }

    /// Mark the spinner as failed. This cancels the spinner.
    pub fn failed(self) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.state = State::Error;
        }
    }

    /// Cancel the spinner with an error.
    pub fn error(self, msg: impl fmt::Display) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.state = State::Error;
            progress.message = Paint::new(format!(
                "{} {} {}",
                progress.message,
                Paint::red("error:"),
                msg
            ));
        }
    }

    /// Cancel the spinner with a warning sign.
    pub fn warn(self) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.state = State::Warn;
        }
    }

    /// Set the spinner's message.
    pub fn message(&mut self, msg: impl fmt::Display) {
        let msg = msg.to_string();

        if let Ok(mut progress) = self.progress.lock() {
            progress.message = Paint::new(msg);
        }
    }
}

/// Create a new spinner with the given message. Sends animation output to `stderr` and success or
/// failure messages to `stdout`. This function handles signals, with there being only one
/// element handling signals at a time, and is a wrapper to [`spinner_to()`].
pub fn spinner(message: impl ToString) -> Spinner {
    let (stdout, stderr) = (io::stdout(), io::stderr());
    if stderr.is_terminal() {
        spinner_to(message, stdout, stderr)
    } else {
        spinner_to(message, stdout, io::sink())
    }
}

/// Create a new spinner with the given message, and send output to the given writers.
///
/// # Signal Handling
///
/// This will install handlers for the spinner until cancelled or dropped, with there
/// being only one element handling signals at a time. If the spinner cannot install
/// handlers, then it will not attempt to install handlers again, and continue running.
pub fn spinner_to(
    message: impl ToString,
    mut completion: impl io::Write + Send + 'static,
    animation: impl io::Write + Send + 'static,
) -> Spinner {
    let message = message.to_string();
    let progress = Arc::new(Mutex::new(Progress::new(Paint::new(message))));
    let (sig_tx, sig_rx) = chan::unbounded();
    let sig_result = signals::install(sig_tx);
    let handle = thread::Builder::new()
        .name(String::from("spinner"))
        .spawn({
            let progress = progress.clone();

            move || {
                let mut animation = termion::cursor::HideCursor::from(animation);

                loop {
                    let Ok(mut progress) = progress.lock() else {
                        break;
                    };
                    // If were unable to install handles, skip signal processing entirely.
                    if sig_result.is_ok() {
                        match sig_rx.try_recv() {
                            Ok(sig) if sig == Signal::Interrupt || sig == Signal::Terminate => {
                                write!(animation, "\r{}", termion::clear::UntilNewline).ok();
                                writeln!(
                                    completion,
                                    "{ERROR_PREFIX} {} {}",
                                    &progress.message,
                                    Paint::red("<canceled>")
                                )
                                .ok();
                                drop(animation);
                                std::process::exit(-1);
                            }
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                    match &mut *progress {
                        Progress {
                            state: State::Running { cursor },
                            message,
                        } => {
                            let spinner = DEFAULT_STYLE[*cursor];

                            write!(
                                animation,
                                "\r{}{spinner} {message}",
                                termion::clear::UntilNewline,
                            )
                            .ok();

                            *cursor += 1;
                            *cursor %= DEFAULT_STYLE.len();
                        }
                        Progress {
                            state: State::Done,
                            message,
                        } => {
                            write!(animation, "\r{}", termion::clear::UntilNewline).ok();
                            writeln!(completion, "{} {message}", Paint::green("✓")).ok();
                            break;
                        }
                        Progress {
                            state: State::Canceled,
                            message,
                        } => {
                            write!(animation, "\r{}", termion::clear::UntilNewline).ok();
                            writeln!(
                                completion,
                                "{ERROR_PREFIX} {message} {}",
                                Paint::red("<canceled>")
                            )
                            .ok();
                            break;
                        }
                        Progress {
                            state: State::Warn,
                            message,
                        } => {
                            write!(animation, "\r{}", termion::clear::UntilNewline).ok();
                            writeln!(completion, "{WARNING_PREFIX} {message}").ok();
                            break;
                        }
                        Progress {
                            state: State::Error,
                            message,
                        } => {
                            write!(animation, "\r{}", termion::clear::UntilNewline).ok();
                            writeln!(completion, "{ERROR_PREFIX} {message}").ok();
                            break;
                        }
                    }
                    drop(progress);
                    thread::sleep(DEFAULT_TICK);
                }
                if sig_result.is_ok() {
                    let _ = signals::uninstall();
                }
            }
        })
        // SAFETY: Only panics if the thread name contains `null` bytes, which isn't the case here.
        .unwrap();

    Spinner {
        progress,
        handle: ManuallyDrop::new(handle),
    }
}

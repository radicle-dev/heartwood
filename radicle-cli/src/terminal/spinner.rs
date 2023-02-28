use std::io::Write;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use std::{fmt, io, thread, time};

use crate::terminal::io::{ERROR_PREFIX, WARNING_PREFIX};
use crate::terminal::Paint;

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

/// Create a new spinner with the given message.
pub fn spinner(message: impl ToString) -> Spinner {
    let message = message.to_string();
    let progress = Arc::new(Mutex::new(Progress::new(Paint::new(message))));
    let handle = thread::spawn({
        let progress = progress.clone();

        move || {
            let mut stdout = io::stdout();
            let mut stderr = termion::cursor::HideCursor::from(io::stderr());

            loop {
                let Ok(mut progress) = progress.lock() else {
                    break;
                };
                match &mut *progress {
                    Progress {
                        state: State::Running { cursor },
                        message,
                    } => {
                        let spinner = DEFAULT_STYLE[*cursor];

                        write!(
                            stderr,
                            "{}{}{spinner} {message}",
                            termion::cursor::Save,
                            termion::clear::AfterCursor,
                        )
                        .ok();

                        write!(stderr, "{}", termion::cursor::Restore).ok();

                        *cursor += 1;
                        *cursor %= DEFAULT_STYLE.len();
                    }
                    Progress {
                        state: State::Done,
                        message,
                    } => {
                        write!(stderr, "{}", termion::clear::AfterCursor).ok();
                        writeln!(stdout, "{} {message}", Paint::green("✓")).ok();
                        break;
                    }
                    Progress {
                        state: State::Canceled,
                        message,
                    } => {
                        write!(stderr, "{}", termion::clear::AfterCursor).ok();
                        writeln!(
                            stdout,
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
                        writeln!(stdout, "{WARNING_PREFIX} {message}").ok();
                        break;
                    }
                    Progress {
                        state: State::Error,
                        message,
                    } => {
                        writeln!(stdout, "{ERROR_PREFIX} {message}").ok();
                        break;
                    }
                }
                drop(progress);
                thread::sleep(DEFAULT_TICK);
            }
        }
    });

    Spinner {
        progress,
        handle: ManuallyDrop::new(handle),
    }
}

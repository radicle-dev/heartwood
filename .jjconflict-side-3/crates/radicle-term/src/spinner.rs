use std::io::IsTerminal;
use std::mem::ManuallyDrop;
use std::sync::{Arc, LazyLock, Mutex};
use std::{fmt, io, thread, time};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::{Paint, PaintTarget};

/// How much time to wait between spinner animation updates.
pub const DEFAULT_TICK: time::Duration = time::Duration::from_millis(120);
/// The spinner animation strings.
pub const DEFAULT_STYLE: [Paint<&'static str>; 4] = [
    Paint::magenta("◢"),
    Paint::cyan("◣"),
    Paint::magenta("◤"),
    Paint::blue("◥"),
];
static TEMPLATE: LazyLock<ProgressStyle> =
    LazyLock::new(|| ProgressStyle::with_template("{spinner:.blue} {msg}").unwrap());

impl From<PaintTarget> for ProgressDrawTarget {
    fn from(value: PaintTarget) -> Self {
        match value {
            PaintTarget::Stdout => ProgressDrawTarget::stdout(),
            PaintTarget::Stderr => ProgressDrawTarget::stderr(),
            PaintTarget::Hidden => ProgressDrawTarget::hidden(),
        }
    }
}

enum State {
    Running,
    Canceled,
    Done,
    Warn,
    Error,
}

struct Progress {
    state: State,
    message: Paint<String>,
}

impl Progress {
    fn new(message: Paint<String>) -> Self {
        Self {
            state: State::Running,
            message,
        }
    }
}

/// A progress spinner.
pub struct Spinner {
    progress: Arc<Mutex<Progress>>,
    handle: ManuallyDrop<thread::JoinHandle<()>>,
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Ok(mut progress) = self.progress.lock() {
            if let State::Running = progress.state {
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
    if io::stderr().is_terminal() {
        spinner_to(message, PaintTarget::Stderr, PaintTarget::Stdout)
    } else {
        spinner_to(message, PaintTarget::Hidden, PaintTarget::Stdout)
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
    progress_target: PaintTarget,
    completion_target: PaintTarget,
) -> Spinner {
    let message = message.to_string();
    let progress = Arc::new(Mutex::new(Progress::new(Paint::new(message.clone()))));

    #[cfg(unix)]
    let (sig_tx, sig_rx) = crossbeam_channel::unbounded();

    #[cfg(unix)]
    let sig_result = radicle_signals::install(sig_tx);

    let handle = thread::Builder::new()
        .name(String::from("spinner"))
        .spawn({
            let progress = progress.clone();
            let spinner = ProgressBar::new_spinner();

            spinner.set_draw_target(progress_target.into());
            spinner.set_message(message.to_string());
            spinner.set_style(TEMPLATE.clone().tick_strings(&[
                DEFAULT_STYLE[0].to_string().as_str(),
                DEFAULT_STYLE[1].to_string().as_str(),
                DEFAULT_STYLE[2].to_string().as_str(),
                DEFAULT_STYLE[3].to_string().as_str(),
            ]));

            move || {
                loop {
                    let Ok(mut progress) = progress.lock() else {
                        break;
                    };
                    // If were unable to install handles, skip signal processing entirely.
                    #[cfg(unix)]
                    if sig_result.is_ok() {
                        match sig_rx.try_recv() {
                            Ok(sig)
                                if sig == radicle_signals::Signal::Interrupt
                                    || sig == radicle_signals::Signal::Terminate =>
                            {
                                spinner.finish_and_clear();
                                writeln!(
                                    completion_target.writer(),
                                    "{} {message} {}",
                                    super::PREFIX_ERROR,
                                    Paint::red("<canceled>")
                                )
                                .ok();
                                std::process::exit(-1);
                            }
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                    match &mut *progress {
                        Progress {
                            state: State::Running,
                            message,
                        } => {
                            spinner.set_message(message.to_string());
                            spinner.inc(1);
                        }

                        Progress {
                            state: State::Done,
                            message,
                        } => {
                            spinner.finish_and_clear();
                            writeln!(
                                completion_target.writer(),
                                "{} {message}",
                                super::PREFIX_SUCCESS
                            )
                            .ok();
                            break;
                        }

                        Progress {
                            state: State::Canceled,
                            message,
                        } => {
                            spinner.finish_and_clear();
                            writeln!(
                                completion_target.writer(),
                                "{} {message} {}",
                                super::PREFIX_ERROR,
                                Paint::red("<canceled>")
                            )
                            .ok();
                            break;
                        }

                        Progress {
                            state: State::Warn,
                            message,
                        } => {
                            spinner.finish_and_clear();
                            writeln!(
                                completion_target.writer(),
                                "{} {message}",
                                super::PREFIX_WARNING
                            )
                            .ok();
                            break;
                        }

                        Progress {
                            state: State::Error,
                            message,
                        } => {
                            spinner.finish_and_clear();
                            writeln!(
                                completion_target.writer(),
                                "{} {message}",
                                super::PREFIX_ERROR
                            )
                            .ok();
                            break;
                        }
                    }
                    drop(progress);
                    thread::sleep(DEFAULT_TICK);
                }

                #[cfg(unix)]
                if sig_result.is_ok() {
                    let _ = radicle_signals::uninstall();
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

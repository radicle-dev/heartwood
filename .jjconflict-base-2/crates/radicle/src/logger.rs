//! Logging module.
//!
//! For test logging see [`mod@test`].

#[cfg(feature = "test")]
pub mod test;

use std::io;
use std::io::Write;

use chrono::prelude::*;
use colored::*;
use log::{Level, Log, Metadata, Record};

/// A logger that logs to `stdout`.
pub struct Logger {
    level: Level,
}

impl Logger {
    pub fn new(level: Level) -> Self {
        Self { level }
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let target = record.target();

            let message = format!(
                "{:<5} {:<8} {}",
                record.level(),
                target.cyan(),
                record.args()
            );

            let message = format!(
                "{} {}",
                Local::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                message,
            );

            let message = match record.level() {
                Level::Error => message.red(),
                Level::Warn => message.yellow(),
                Level::Info => message.normal(),
                Level::Debug => message.dimmed(),
                Level::Trace => message.white().dimmed(),
            };
            writeln!(&mut io::stdout(), "{message}").expect("write shouldn't fail");
        }
    }

    fn flush(&self) {}
}

/// A logger that logs to `stderr`.
pub struct StderrLogger {
    level: Level,
}

impl StderrLogger {
    pub fn new(level: Level) -> Self {
        Self { level }
    }
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let message = format!(
                "{:<5} {:<8} {}",
                record.level(),
                record.target(),
                record.args()
            );
            writeln!(&mut io::stderr(), "{message}").expect("write shouldn't fail");
        }
    }

    fn flush(&self) {}
}

/// Get the level set by the environment variable `RUST_LOG`, if
/// present.
pub fn env_level() -> Option<Level> {
    let level = std::env::var("RUST_LOG").ok()?;
    level.parse().ok()
}

//! Logging module.
//!
//! For test logging see [`mod@test`].

#[cfg(feature = "test")]
pub mod test;

use std::io::{self, Write};

use chrono::prelude::*;
use colored::*;
use log::{Level, Log, Metadata, Record, SetLoggerError};

struct Logger {
    level: Level,
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

            writeln!(io::stdout(), "{message}").expect("write shouldn't fail");
        }
    }

    fn flush(&self) {}
}

/// Initialize a new logger.
pub fn init(level: Level) -> Result<(), SetLoggerError> {
    let logger = Logger { level };

    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(level.to_level_filter());

    Ok(())
}

/// Get the level set by the environment variable `RUST_LOG`, if
/// present.
pub fn env_level() -> Option<Level> {
    let level = std::env::var("RUST_LOG").ok()?;
    level.parse().ok()
}

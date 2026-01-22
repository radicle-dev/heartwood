use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use log::{Level, Log, Metadata, Record, SetLoggerError};
use regex::Regex;

use radicle_localtime::LocalTime;
use radicle_term::{Color, Paint};

/// A writer that can be shared across threads.
pub type SharedWriter = Arc<Mutex<dyn Write + Send + Sync>>;

/// The Test Logger
/// Logs with Epoch timestamps, "test"/"sim" formatting and regex highlighting
pub struct Logger {
    level: Level,
    pub base58_ref_oid_re: Regex,
    pub base58_re: Regex,
    writer: SharedWriter,
}

/// The Base58 pattern used for Radicle IDs.
const BASE58_REGEX: &str = r"z[1-9A-HJ-NP-Za-km-z]{10,}";

impl Logger {
    pub fn new(level: Level) -> Self {
        Self {
            level,
            // base58: Starts with 'z', base58 chars, 10+ length.
            // ref: Starts with 'refs/', followed by valid ref chars.
            // oid: Hex characters, between 6 and 40 length, with word boundaries. (currently
            // matching timestamps too e.g. `1769096403171`)
            base58_ref_oid_re: Regex::new(&format!(
                r"(?P<base58>{BASE58_REGEX})|(?P<ref>refs/[a-zA-Z0-9/*._-]+)|(?P<oid>\b[0-9a-f]{{6,40}}\b)")
            ).expect("invalid regex"),
            base58_re: Regex::new(BASE58_REGEX).expect("invalid id regex"),
            writer: Arc::new(Mutex::new(io::stdout())),
        }
    }

    /// Create a new logger with a custom writer.
    pub fn with_writer(level: Level, writer: SharedWriter) -> Self {
        let mut logger = Self::new(level);
        logger.writer = writer;
        logger
    }

    pub fn init(self) -> Result<(), SetLoggerError> {
        let level = self.level;
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(level.to_level_filter());
        Ok(())
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let target = record.target();
        let level = record.level();

        // Helper to paint the "plain" parts of the message based on the target/level.
        let paint_plain = |s: &str| -> String {
            match target {
                "test" => Paint::cyan(s).to_string(),
                "sim" => Paint::new(s).bold().to_string(),
                _ => match level {
                    Level::Warn => Paint::yellow(s).underline().to_string(),
                    Level::Error => Paint::red(s).underline().to_string(),
                    _ => Paint::new(s).dim().to_string(),
                },
            }
        };

        let msg = record.args().to_string();
        let mut coloured_msg = String::new();
        let mut last_match = 0;

        // Iterate over the main composite matches
        for caps in self.base58_ref_oid_re.captures_iter(&msg) {
            let whole_match = caps.get(0).unwrap();

            // Paint text BEFORE the match (Plain style)
            coloured_msg.push_str(&paint_plain(&msg[last_match..whole_match.start()]));

            // Handle the match based on which group captured it
            if let Some(m) = caps.name("base58") {
                // Standard Base58 match (not inside a ref)
                let match_str = m.as_str();
                coloured_msg.push_str(&paint_base58(match_str).to_string());
            } else if let Some(m) = caps.name("oid") {
                // Git OID match (RGB from hex with contrast check)
                let oid = m.as_str();
                coloured_msg.push_str(&paint_oid(oid).to_string());
            } else if let Some(m) = caps.name("ref") {
                // Git Ref match
                let ref_str = m.as_str();
                let mut last_ref_idx = 0;

                // Search for Base58 IDs *inside* this ref string
                for id_match in self.base58_re.find_iter(ref_str) {
                    let prefix = &ref_str[last_ref_idx..id_match.start()];
                    coloured_msg.push_str(&Paint::new(prefix).bold().underline().to_string());

                    // Paint the ID itself (Deterministic Colour + Bold)
                    let id = id_match.as_str();
                    coloured_msg.push_str(
                        &colour_for_base58(id)
                            .paint(id)
                            .bold()
                            .underline()
                            .to_string(),
                    );

                    last_ref_idx = id_match.end();
                }

                let suffix = &ref_str[last_ref_idx..];
                coloured_msg.push_str(&Paint::new(suffix).bold().underline().to_string());
            }

            last_match = whole_match.end();
        }

        // Paint the remaining text
        coloured_msg.push_str(&paint_plain(&msg[last_match..]));

        let time = LocalTime::now().as_secs();

        match target {
            "test" => {
                let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
                writeln!(writer, "{} {} {}", time, Paint::cyan("test:"), coloured_msg).ok();
            }
            "sim" => {
                let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
                writeln!(
                    writer,
                    "{} {}  {}",
                    time,
                    Paint::new("sim:").bold(),
                    coloured_msg
                )
                .ok();
            }
            _ => {
                let current = std::thread::current();
                let target_str = format!("{}:", target);

                let prefix = if let Some(name) = current.name() {
                    format!("{} {:<16} {:>10}", time, name, target_str)
                } else {
                    format!("{} {:>10}", time, target_str)
                };
                let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
                writeln!(writer, "{} {}", paint_plain(&prefix), coloured_msg).ok();
            }
        }
    }

    fn flush(&self) {
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        writer.flush().ok();
    }
}

fn paint_base58(s: &str) -> Paint<&str> {
    let colour = colour_for_base58(s);
    colour.paint(s).bold()
}

/// Deterministically pick a colour for a base58 string.
/// NOTE: If the output contains more than base58 strings than the number of colours below,
/// consider switching to the `paint_oid` system.
pub fn colour_for_base58(s: &str) -> Color {
    let mut hash: u32 = 0;
    for b in s.bytes() {
        hash = hash.wrapping_add(b as u32);
    }

    let colours = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::White,
    ];

    colours[(hash as usize) % colours.len()]
}

/// Paint an OID using its first 6 characters as an RGB hex code.
/// Automatically applies a contrasting background if the colour is too bright or too dark.
pub fn paint_oid(oid: &str) -> Paint<&str> {
    if oid.len() < 6 {
        return Paint::yellow(oid);
    }

    let r = u8::from_str_radix(&oid[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&oid[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&oid[4..6], 16).unwrap_or(128);

    // Calculate relative luminance (0.0 to 255.0)
    // Formula: 0.299*R + 0.587*G + 0.114*B
    let luminance = (0.299 * r as f32) + (0.587 * g as f32) + (0.114 * b as f32);

    let colour = Color::RGB(r, g, b);
    let paint = colour.paint(oid);

    // Thresholds: < 40 is very dark, > 215 is very bright.
    if luminance < 40.0 {
        paint.bg(Color::White)
    } else if luminance > 215.0 {
        paint.bg(Color::Black)
    } else {
        paint
    }
}

/// Initialize the logger with the given level.
pub fn init(level: Level) -> Result<(), SetLoggerError> {
    Logger::new(level).init()
}

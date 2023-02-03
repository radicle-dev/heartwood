use log::*;

struct Logger {
    level: Level,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        use colored::Colorize;

        match record.target() {
            "test" => {
                println!("{} {}", "test:".cyan(), record.args().to_string().cyan())
            }
            "sim" => {
                println!("{}  {}", "sim:".bold(), record.args().to_string().bold())
            }
            target => {
                if self.enabled(record.metadata()) {
                    let current = std::thread::current();
                    let msg = format!("{:>12} {}", format!("{target}:"), record.args());
                    let s = if let Some(name) = current.name() {
                        format!("{name:<16} {msg}")
                    } else {
                        msg
                    };
                    match record.level() {
                        log::Level::Warn => {
                            println!("{}", s.yellow());
                        }
                        log::Level::Error => {
                            println!("{}", s.red());
                        }
                        _ => {
                            println!("{}", s.dimmed());
                        }
                    }
                }
            }
        }
    }

    fn flush(&self) {}
}

#[allow(dead_code)]
pub fn init(level: Level) {
    let logger = Logger { level };

    log::set_boxed_logger(Box::new(logger)).ok();
    log::set_max_level(level.to_level_filter());
}

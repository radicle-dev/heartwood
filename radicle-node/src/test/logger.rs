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
                println!("{} {}", "test:".cyan(), record.args().to_string().yellow())
            }
            "sim" => {
                println!("{}  {}", "sim:".bold(), record.args().to_string().bold())
            }
            target => {
                if self.enabled(record.metadata()) {
                    let id = std::thread::current().id();
                    let s = format!("{:?} {:<8} {}", id, format!("{}:", target), record.args());
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

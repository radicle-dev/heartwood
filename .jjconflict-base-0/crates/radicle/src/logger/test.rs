use localtime::LocalTime;
use log::*;

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
        use colored::Colorize;
        let time = LocalTime::now().as_secs();

        match record.target() {
            "test" => {
                println!(
                    "{time} {} {}",
                    "test:".cyan(),
                    record.args().to_string().cyan()
                )
            }
            "sim" => {
                println!(
                    "{time} {}  {}",
                    "sim:".bold(),
                    record.args().to_string().bold()
                )
            }
            target => {
                if self.enabled(record.metadata()) {
                    let current = std::thread::current();
                    let msg = format!("{:>10} {}", format!("{target}:"), record.args());
                    let time = LocalTime::now().as_secs();
                    let s = if let Some(name) = current.name() {
                        format!("{time} {name:<16} {msg}")
                    } else {
                        format!("{time} {msg}")
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

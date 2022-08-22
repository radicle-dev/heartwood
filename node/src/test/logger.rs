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
                println!(
                    "{} {}",
                    "test:".yellow(),
                    record.args().to_string().yellow()
                )
            }
            "sim" => {
                println!("{}  {}", "sim:".bold(), record.args().to_string().bold())
            }
            target => {
                if self.enabled(record.metadata()) {
                    let s = format!("{:<8} {}", format!("{}:", target), record.args());
                    println!("{}", s.dimmed());
                }
            }
        }
    }

    fn flush(&self) {}
}

pub fn init(level: Level) {
    let logger = Logger { level };

    log::set_boxed_logger(Box::new(logger)).ok();
    log::set_max_level(level.to_level_filter());
}

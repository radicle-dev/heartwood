use std::env;
use std::process;

use radicle::version::Version;

pub const VERSION: Version = Version {
    name: "git-remote-rad",
    commit: env!("GIT_HEAD"),
    version: env!("RADICLE_VERSION"),
    timestamp: env!("SOURCE_DATE_EPOCH"),
};

fn main() {
    let mut args = env::args();

    if let Some(lvl) = radicle::logger::env_level() {
        let logger = radicle::logger::StderrLogger::new(lvl);
        log::set_boxed_logger(Box::new(logger))
            .expect("no other logger should have been set already");
        log::set_max_level(lvl.to_level_filter());
    }
    if args.nth(1).as_deref() == Some("--version") {
        if let Err(e) = VERSION.write(std::io::stdout()) {
            eprintln!("error: {e}");
            process::exit(1);
        };
        process::exit(0);
    }

    let profile = match radicle::Profile::load() {
        Ok(profile) => profile,
        Err(err) => {
            eprintln!("error: couldn't load profile: {err}");
            process::exit(1);
        }
    };

    if let Err(err) = radicle_remote_helper::run(profile) {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

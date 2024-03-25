use std::env;
use std::process;

use radicle::version::Version;

pub const VERSION: Version = Version {
    name: "git-remote-rad",
    commit: env!("GIT_HEAD"),
    version: env!("RADICLE_VERSION"),
    timestamp: env!("GIT_COMMIT_TIME"),
};

fn main() {
    let mut args = env::args();

    if let Some(lvl) = radicle::logger::env_level() {
        radicle::logger::set(radicle::logger::StderrLogger::new(lvl), lvl).ok();
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

use std::env;
use std::process;

use radicle::version;

pub const NAME: &str = "git-remote-rad";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HEAD: &str = env!("GIT_HEAD");

fn main() {
    let mut args = env::args();

    if args.nth(1).as_deref() == Some("--version") {
        if let Err(e) = version::print(std::io::stdout(), NAME, VERSION, GIT_HEAD) {
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

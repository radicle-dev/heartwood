use std::process;

use radicle_term as term;

pub const NAME: &str = "radicle-tui";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HEAD: &str = env!("GIT_HEAD");
pub const FPS: u64 = 60;

pub const HELP: &str = r#"
Usage

    radicle-tui [<option>...]

Options

    --version       Print version
    --help          Print help

"#;

struct Options;

impl Options {
    fn from_env() -> Result<Self, anyhow::Error> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();

        while let Some(arg) = parser.next()? {
            match arg {
                Long("version") => {
                    println!("{NAME} {VERSION}+{GIT_HEAD}");
                    process::exit(0);
                }
                Long("help") => {
                    println!("{HELP}");
                    process::exit(0);
                }
                _ => anyhow::bail!(arg.unexpected()),
            }
        }

        Ok(Self {})
    }
}

fn execute() -> anyhow::Result<()> {
    let _ = Options::from_env()?;
    Ok(())
}

fn main() {
    if let Err(err) = execute() {
        term::error(format!("Error: rad-tui: {err}"));
        process::exit(1);
    }
}

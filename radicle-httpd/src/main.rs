use std::num::NonZeroUsize;
use std::{collections::HashMap, process};

use radicle::prelude::RepoId;
use radicle_httpd as httpd;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = parse_options()?;

    // SAFETY: The logger is only initialized once.
    httpd::logger::init().unwrap();
    tracing::info!("version {}-{}", env!("RADICLE_VERSION"), env!("GIT_HEAD"));

    match httpd::run(options).await {
        Ok(()) => {}
        Err(err) => {
            tracing::error!("Fatal: {:#}", err);
            process::exit(1);
        }
    }
    Ok(())
}

/// Parse command-line arguments into HTTP options.
fn parse_options() -> Result<httpd::Options, lexopt::Error> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut listen = None;
    let mut aliases = HashMap::new();
    let mut cache = Some(httpd::DEFAULT_CACHE_SIZE);

    while let Some(arg) = parser.next()? {
        match arg {
            Long("listen") => {
                let addr = parser.value()?.parse()?;
                listen = Some(addr);
            }
            Long("alias") | Short('a') => {
                let alias: String = parser.value()?.parse()?;
                let id: RepoId = parser.value()?.parse()?;

                aliases.insert(alias, id);
            }
            Long("cache") => {
                let size = parser.value()?.parse()?;
                cache = NonZeroUsize::new(size);
            }
            Long("help") | Short('h') => {
                println!("usage: radicle-httpd [--listen <addr>] [--alias <name> <rid>] [--cache <size>]..");
                process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(httpd::Options {
        aliases,
        listen: listen.unwrap_or_else(|| ([0, 0, 0, 0], 8080).into()),
        cache,
    })
}

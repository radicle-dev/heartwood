use std::process;

use radicle_httpd as httpd;
use tracing::dispatcher::Dispatch;

#[cfg(feature = "logfmt")]
mod logger {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::EnvFilter;

    pub fn subscriber() -> impl tracing::Subscriber {
        tracing_subscriber::Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(tracing_logfmt::layer())
    }
}

#[cfg(not(feature = "logfmt"))]
mod logger {
    pub fn subscriber() -> impl tracing::Subscriber {
        tracing_subscriber::FmtSubscriber::new()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = parse_options()?;

    tracing::dispatcher::set_global_default(Dispatch::new(logger::subscriber()))
        .expect("Global logger hasn't already been set");

    tracing::info!("version {}-{}", env!("CARGO_PKG_VERSION"), env!("GIT_HEAD"));

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

    while let Some(arg) = parser.next()? {
        match arg {
            Long("listen") => {
                let addr = parser.value()?.parse()?;
                listen = Some(addr);
            }
            Long("help") => {
                println!("usage: radicle-httpd [--listen <addr>]");
                process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(httpd::Options {
        listen: listen.unwrap_or_else(|| ([0, 0, 0, 0], 8080).into()),
    })
}

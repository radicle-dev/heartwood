use std::{net, process};

use tracing::dispatcher::Dispatch;

use radicle_httpd as httpd;

#[derive(Debug)]
pub struct Options {
    pub listen: net::SocketAddr,
}

impl Options {
    fn from_env() -> Result<Self, lexopt::Error> {
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
        Ok(Self {
            listen: listen.unwrap_or_else(|| ([0, 0, 0, 0], 8080).into()),
        })
    }
}

impl From<Options> for httpd::Options {
    fn from(other: Options) -> Self {
        Self {
            listen: other.listen,
        }
    }
}

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
    let options = Options::from_env()?;

    tracing::dispatcher::set_global_default(Dispatch::new(logger::subscriber()))
        .expect("Global logger hasn't already been set");

    tracing::info!("version {}-{}", env!("CARGO_PKG_VERSION"), env!("GIT_HEAD"));

    match httpd::run(options.into()).await {
        Ok(()) => {}
        Err(err) => {
            tracing::error!("Fatal: {:#}", err);
            process::exit(1);
        }
    }
    Ok(())
}

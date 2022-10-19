use std::thread;
use std::{net, process};

use anyhow::Context as _;

use radicle_node::logger;
use radicle_node::prelude::Address;
use radicle_node::{client, control, git, service};

type Reactor = nakamoto_net_poll::Reactor<net::TcpStream>;

#[derive(Debug)]
struct Options {
    connect: Vec<Address>,
    listen: Vec<net::SocketAddr>,
    git_url: git::Url,
}

impl Options {
    fn from_env() -> Result<Self, lexopt::Error> {
        use lexopt::prelude::*;
        let mut parser = lexopt::Parser::from_env();
        let mut connect = Vec::new();
        let mut listen = Vec::new();
        let mut git_url = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("connect") => {
                    let addr = parser.value()?.parse()?;
                    connect.push(addr);
                }
                Long("listen") => {
                    let addr = parser.value()?.parse()?;
                    listen.push(addr);
                }
                Long("git-url") => {
                    let url = git::Url::from_bytes(parser.value()?.into_string()?.as_bytes())
                        .map_err(|e| format!("invalid URL: {}", e))?;
                    git_url = Some(url);
                }
                Long("help") => {
                    println!("usage: radicle-node [--connect <addr>]..");
                    process::exit(0);
                }
                _ => return Err(arg.unexpected()),
            }
        }
        Ok(Self {
            connect,
            listen,
            git_url: git_url.ok_or("a Git URL must be specified with `--git-url`")?,
        })
    }
}

fn main() -> anyhow::Result<()> {
    logger::init(log::Level::Debug)?;

    let options = Options::from_env()?;
    let profile = radicle::Profile::load().context("Failed to load node profile")?;
    let socket = profile.socket();
    let client = client::Client::<Reactor>::new(profile).context("Failed to initialize client")?;
    let handle = client.handle();
    let config = client::Config {
        service: service::Config {
            connect: options.connect,
            git_url: options.git_url,
            ..service::Config::default()
        },
        listen: options.listen,
    };

    let t1 = thread::spawn(move || control::listen(socket, handle));
    let t2 = thread::spawn(move || client.run(config));

    t1.join().unwrap()?;
    t2.join().unwrap()?;

    Ok(())
}

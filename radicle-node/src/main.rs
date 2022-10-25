use std::thread;
use std::{net, process};

use anyhow::Context as _;

use radicle_node::logger;
use radicle_node::prelude::Address;
use radicle_node::{client, control, service};

type Reactor = nakamoto_net_poll::Reactor<net::TcpStream>;

#[derive(Debug)]
struct Options {
    connect: Vec<Address>,
    listen: Vec<net::SocketAddr>,
}

impl Options {
    fn from_env() -> Result<Self, lexopt::Error> {
        use lexopt::prelude::*;
        let mut parser = lexopt::Parser::from_env();
        let mut connect = Vec::new();
        let mut listen = Vec::new();

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
                Long("help") => {
                    println!("usage: radicle-node [--connect <addr>]..");
                    process::exit(0);
                }
                _ => return Err(arg.unexpected()),
            }
        }
        Ok(Self { connect, listen })
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

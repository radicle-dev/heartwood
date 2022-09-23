use std::thread;
use std::{env, net};

use radicle_node::{client, control};

type Reactor = nakamoto_net_poll::Reactor<net::TcpStream>;

fn main() -> anyhow::Result<()> {
    let profile = radicle::Profile::load()?;
    let client = client::Client::<Reactor>::new(profile)?;
    let handle = client.handle();
    let config = client::Config::default();
    let socket = env::var("RAD_SOCKET").unwrap_or_else(|_| control::DEFAULT_SOCKET_NAME.to_owned());

    let t1 = thread::spawn(move || control::listen(socket, handle));
    let t2 = thread::spawn(move || client.run(config));

    t1.join().unwrap()?;
    t2.join().unwrap()?;

    Ok(())
}

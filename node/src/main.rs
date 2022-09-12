use std::path::Path;
use std::thread;
use std::{env, net};

use radicle_node::crypto::{PublicKey, Signature, Signer};
use radicle_node::{client, control};

type Reactor = nakamoto_net_poll::Reactor<net::TcpStream>;

struct FailingSigner {}

impl Signer for FailingSigner {
    fn public_key(&self) -> &PublicKey {
        panic!("Failing signer always fails!");
    }

    fn sign(&self, _msg: &[u8]) -> Signature {
        panic!("Failing signer always fails!");
    }
}

fn main() -> anyhow::Result<()> {
    let signer = FailingSigner {};
    let client = client::Client::<Reactor, _>::new(Path::new("."), signer)?;
    let handle = client.handle();
    let config = client::Config::default();
    let socket = env::var("RAD_SOCKET").unwrap_or_else(|_| control::DEFAULT_SOCKET_NAME.to_owned());

    let t1 = thread::spawn(move || control::listen(socket, handle));
    let t2 = thread::spawn(move || client.run(config));

    t1.join().unwrap()?;
    t2.join().unwrap()?;

    Ok(())
}

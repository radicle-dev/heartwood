use std::net;
use std::path::Path;

use radicle_node::client;
use radicle_node::crypto::{PublicKey, Signature, Signer};

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
    let client = client::Client::<Reactor>::new(Path::new("."), signer)?;

    client.run(client::Config::default())?;

    Ok(())
}

use radicle::{crypto, crypto::ssh};
use std::io::prelude::*;
use std::{env, io};

fn main() -> anyhow::Result<()> {
    let profile = radicle::Profile::load()?;

    println!("({})", ssh::fmt::key(profile.id()));

    match env::args().nth(1).as_deref() {
        Some("add") => {
            ssh::agent::register(&profile.signer.secret)?;
            println!("ok");
        }
        Some("remove") => {
            ssh::agent::connect()?.remove_identity(profile.id())?;
            println!("ok");
        }
        Some("remove-all") => {
            ssh::agent::connect()?.remove_all_identities()?;
            println!("ok");
        }
        Some("sign") => {
            let mut stdin = Vec::new();
            io::stdin().read_to_end(&mut stdin)?;

            let mut agent = ssh::agent::connect()?;
            let sig = agent.sign_request(profile.id(), stdin.into())?;
            let sig = crypto::Signature::from(sig);

            println!("{}", &sig);
        }
        Some(other) => {
            anyhow::bail!("Unknown command `{}`", other);
        }
        None => {}
    }

    Ok(())
}

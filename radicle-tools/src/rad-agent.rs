use anyhow::{anyhow, Context as _};
use radicle::{crypto, crypto::ssh};
use std::io::prelude::*;
use std::{env, io};

fn main() -> anyhow::Result<()> {
    let profile = radicle::Profile::load()?;
    let mut agent = ssh::agent::Agent::connect()?;

    println!("key: {}", ssh::fmt::key(profile.id()));
    println!("hash: {}", ssh::fmt::fingerprint(profile.id()));

    match env::args().nth(1).as_deref() {
        Some("add") => {
            print!("passphrase: ");
            io::stdout().flush()?;

            let mut passphrase = String::new();
            io::stdin().lock().read_line(&mut passphrase)?;

            let passphrase = passphrase.trim().to_owned().into();
            let secret = profile
                .keystore
                .secret_key(passphrase)?
                .ok_or_else(|| anyhow!("Key not found in {:?}", profile.keystore.path()))?;

            agent.register(&secret)?;
            println!("ok");
        }
        Some("remove") => {
            agent.remove_identity(profile.id())?;
            println!("ok");
        }
        Some("remove-all") => {
            agent.remove_all_identities()?;
            println!("ok");
        }
        Some("sign") => {
            let mut stdin = Vec::new();
            io::stdin().read_to_end(&mut stdin)?;

            let sig = agent.sign(profile.id(), &stdin).context("Signing failed")?;
            let sig = crypto::Signature::from(sig);

            println!("{}", &sig);
        }
        Some(other) => {
            anyhow::bail!("Unknown command `{}`", other);
        }
        None => {
            if agent.signer(profile.public_key).is_ready()? {
                println!("ready: yes");
            } else {
                println!("ready: no");
            }
        }
    }

    Ok(())
}

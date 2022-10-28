use radicle::profile;
use radicle::Profile;

fn main() -> anyhow::Result<()> {
    let profile = match Profile::load() {
        Ok(v) => v,
        Err(profile::Error::NotFound(_)) => {
            let keypair = radicle::crypto::KeyPair::generate();
            radicle::crypto::ssh::agent::register(&keypair.sk)?;
            radicle::Profile::init(keypair)?
        }
        Err(err) => anyhow::bail!(err),
    };
    println!("id: {}", profile.id());
    println!("home: {}", profile.home.display());

    Ok(())
}

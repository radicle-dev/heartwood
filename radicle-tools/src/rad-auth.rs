fn main() -> anyhow::Result<()> {
    let keypair = radicle::crypto::KeyPair::generate();
    radicle::ssh::agent::register(&keypair.sk)?;

    let profile = radicle::Profile::init(keypair)?;

    println!("id: {}", profile.id());
    println!("home: {}", profile.home.display());

    Ok(())
}

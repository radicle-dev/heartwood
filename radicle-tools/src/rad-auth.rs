fn main() -> anyhow::Result<()> {
    let keypair = radicle::crypto::KeyPair::generate();
    let profile = radicle::Profile::init(keypair)?;

    println!("id: {}", profile.id());
    println!("home: {}", profile.home.display());

    Ok(())
}

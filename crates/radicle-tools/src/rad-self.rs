fn main() -> anyhow::Result<()> {
    let profile = radicle::Profile::load()?;

    println!("id: {}", profile.id());
    println!("key: {}", radicle::crypto::ssh::fmt::key(profile.id()));
    println!(
        "fingerprint: {}",
        radicle::crypto::ssh::fmt::fingerprint(profile.id())
    );
    println!("home: {}", profile.home().path().display());

    Ok(())
}

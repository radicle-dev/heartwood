fn main() -> anyhow::Result<()> {
    let profile = radicle::Profile::load()?;

    println!("id: {}", profile.id());
    println!("key: {}", radicle::ssh::fmt::key(profile.id()));
    println!(
        "fingerprint: {}",
        radicle::ssh::fmt::fingerprint(profile.id())
    );
    println!("home: {}", profile.home.display());

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let profile = radicle::Profile::load()?;

    println!("id: {}", profile.id());
    println!("home: {}", profile.home.display());

    Ok(())
}

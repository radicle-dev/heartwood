use radicle::profile;
use radicle::profile::{Error, Profile};

fn main() -> anyhow::Result<()> {
    let profile = match Profile::load() {
        Ok(profile) => profile,
        Err(Error::NotFound(_)) => Profile::init(profile::home()?, "radicle".to_owned())?,
        Err(err) => anyhow::bail!(err),
    };

    println!("id: {}", profile.id());
    println!("home: {}", profile.home().display());

    Ok(())
}

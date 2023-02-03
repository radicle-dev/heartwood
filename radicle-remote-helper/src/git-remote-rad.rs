fn main() {
    let profile = match radicle::Profile::load() {
        Ok(profile) => profile,
        Err(err) => {
            eprintln!("fatal: couldn't load profile: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = radicle_remote_helper::run(profile) {
        eprintln!("fatal: {err}");
        std::process::exit(1);
    }
}

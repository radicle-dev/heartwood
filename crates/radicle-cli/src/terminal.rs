pub(crate) mod args;
pub(crate) use args::Error;

pub mod format;
pub mod io;
pub use io::signer;
pub mod cob;
pub mod comment;
pub mod highlight;
pub mod issue;
pub mod json;
pub mod patch;
pub mod upload_pack;

pub use radicle_term::*;

use radicle::profile::{Home, Profile};

/// Context passed to all commands.
pub trait Context {
    /// Return the currently active profile, or an error if no profile is active.
    fn profile(&self) -> Result<Profile, anyhow::Error>;
    /// Return the Radicle home.
    fn home(&self) -> Result<Home, std::io::Error>;
}

impl Context for Profile {
    fn profile(&self) -> Result<Profile, anyhow::Error> {
        Ok(self.clone())
    }

    fn home(&self) -> Result<Home, std::io::Error> {
        Ok(self.home.clone())
    }
}

/// Gets the default profile. Fails if there is no profile.
pub struct DefaultContext;

impl Context for DefaultContext {
    fn home(&self) -> Result<Home, std::io::Error> {
        radicle::profile::home()
    }

    fn profile(&self) -> Result<Profile, anyhow::Error> {
        match Profile::load() {
            Ok(profile) => Ok(profile),
            Err(radicle::profile::Error::NotFound(path)) => Err(args::Error::WithHint {
                err: anyhow::anyhow!("Radicle profile not found in '{}'.", path.display()),
                hint: "To setup your radicle profile, run `rad auth`.",
            }
            .into()),
            Err(radicle::profile::Error::LoadConfig(e)) => Err(e.into()),
            Err(e) => Err(anyhow::anyhow!("Could not load radicle profile: {e}")),
        }
    }
}

pub fn fail(error: &anyhow::Error) {
    let err = error.to_string();
    let err = err.trim_end();

    for line in err.lines() {
        io::error(line);
    }

    // Catch common node errors, and offer a hint.
    if let Some(e) = error.downcast_ref::<radicle::node::Error>() {
        if e.is_connection_err() {
            io::hint("to start your node, run `rad node start`.");
        }
    }
    if let Some(Error::WithHint { hint, .. }) = error.downcast_ref::<Error>() {
        io::hint(hint);
    }
}

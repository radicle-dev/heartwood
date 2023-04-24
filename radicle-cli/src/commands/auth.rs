#![allow(clippy::or_fun_call)]
use std::ffi::OsString;

use anyhow::anyhow;

use radicle::crypto::ssh;
use radicle::crypto::ssh::Passphrase;
use radicle::profile::env::RAD_PASSPHRASE;
use radicle::{profile, Profile};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "auth",
    description: "Manage identities and profiles",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad auth [<option>...]

    A passphrase may be given via the environment variable `RAD_PASSPHRASE` or
    via the standard input stream if `--stdin` is used. Using either of these
    methods disables the passphrase prompt.

Options

    --stdin                 Read passphrase from stdin (default: false)
    --help                  Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub stdin: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut stdin = false;
        let mut parser = lexopt::Parser::from_args(args);

        while let Some(arg) = parser.next()? {
            match arg {
                Long("stdin") => {
                    stdin = true;
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((Options { stdin }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    match ctx.profile() {
        Ok(profile) => authenticate(&profile, options),
        Err(_) => init(options),
    }
}

/// Connect to the SSH Agent or None if its not present.
#[inline]
fn connect_ssh_agent() -> Result<Option<ssh::agent::Agent>, ssh::agent::Error> {
    match ssh::agent::Agent::connect() {
        Ok(agent) => Ok(Some(agent)),
        Err(ssh::agent::Error::EnvVar("SSH_AUTH_SOCK")) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn init(options: Options) -> anyhow::Result<()> {
    term::headline(format!(
        "Initializing your {} 🌱 identity",
        term::format::highlight("radicle")
    ));

    if let Ok(version) = radicle::git::version() {
        if version < radicle::git::VERSION_REQUIRED {
            term::warning(&format!(
                "Your git version is unsupported, please upgrade to {} or later",
                radicle::git::VERSION_REQUIRED,
            ));
            term::blank();
        }
    } else {
        anyhow::bail!("Error retrieving git version; please check your installation");
    }

    let home = profile::home()?;
    let passphrase = if options.stdin {
        term::passphrase_stdin()
    } else {
        term::passphrase_confirm("Enter a passphrase:", RAD_PASSPHRASE)
    }?;
    let spinner = term::spinner("Creating your Ed25519 keypair...");
    let profile = Profile::init(home, passphrase.clone())?;
    spinner.finish();

    if let Some(mut agent) = connect_ssh_agent()? {
        let spinner = term::spinner("Adding your radicle key to ssh-agent...");
        if register(&mut agent, &profile, passphrase).is_ok() {
            spinner.finish();
        } else {
            spinner.warn();
        }
    }

    term::success!(
        "Your Radicle ID is {}. This identifies your device.",
        term::format::highlight(profile.did())
    );

    term::blank();
    term::tip!(
        "To create a radicle project, run {} from a git repository.",
        term::format::secondary("`rad init`")
    );

    Ok(())
}

pub fn authenticate(profile: &Profile, options: Options) -> anyhow::Result<()> {
    let agent = connect_ssh_agent()?;
    let use_ssh_agent = match agent {
        Some(agent) => {
            if agent.signer(profile.public_key).is_ready()? {
                term::success!("Signing key already in ssh-agent");
                return Ok(());
            }
            true
        }
        None => false,
    };

    term::headline(format!(
        "🌱 Authenticating as {}",
        term::format::Identity::new(profile).styled()
    ));
    if use_ssh_agent {
        // TODO: We should show the spinner on the passphrase prompt,
        // otherwise it seems like the passphrase is valid even if it isn't.
        term::warning("Adding your radicle key to ssh-agent...");
        let passphrase = if options.stdin {
            term::passphrase_stdin()
        } else {
            term::passphrase(RAD_PASSPHRASE)
        }?;
        let spinner = term::spinner("Unlocking...");
        let mut agent = connect_ssh_agent()?.unwrap();
        register(&mut agent, profile, passphrase)?;
        spinner.finish();

        term::success!("Radicle key added to ssh-agent");
    } else if let Some(passphrase) = profile::env::passphrase() {
        ssh::keystore::MemorySigner::load(&profile.keystore, passphrase)?;
        term::success!("Ok");
    } else {
        anyhow::bail!("Set the 'RAD_PASSPHRASE' environment variable to use without ssh-agent.");
    }

    Ok(())
}

/// Register key with ssh-agent.
pub fn register(
    agent: &mut ssh::agent::Agent,
    profile: &Profile,
    passphrase: Passphrase,
) -> anyhow::Result<()> {
    let secret = profile
        .keystore
        .secret_key(passphrase)?
        .ok_or_else(|| anyhow!("Key not found in {:?}", profile.keystore.path()))?;
    agent.register(&secret)?;

    Ok(())
}

#![allow(clippy::or_fun_call)]
use std::ffi::OsString;

use anyhow::anyhow;

use radicle::crypto::ssh;
use radicle::profile;
use radicle::Profile;

use crate::git;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "auth",
    description: "Manage radicle identities and profiles",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad auth [<options>...]

    A passphrase may be given via the environment variable `RAD_PASSPHRASE` or
    via the standard input stream if `--stdin` is used. Using one of these
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

pub fn init(options: Options) -> anyhow::Result<()> {
    term::headline("Initializing your 🌱 profile and identity");

    if git::check_version().is_err() {
        term::warning(&format!(
            "Your git version is unsupported, please upgrade to {} or later",
            git::VERSION_REQUIRED,
        ));
        term::blank();
    }

    let home = profile::home()?;
    let passphrase = term::read_passphrase(options.stdin, true)?;
    let spinner = term::spinner("Creating your 🌱 Ed25519 keypair...");
    let profile = Profile::init(home, passphrase.as_str())?;
    spinner.finish();

    term::success!(
        "Profile {} created.",
        term::format::highlight(&profile.id().to_string())
    );

    term::blank();
    term::info!(
        "Your radicle Node ID is {}. This identifies your device.",
        term::format::highlight(&profile.id().to_string())
    );

    term::blank();
    term::tip!(
        "To create a radicle project, run {} from a git repository.",
        term::format::secondary("`rad init`")
    );

    Ok(())
}

pub fn authenticate(profile: &Profile, options: Options) -> anyhow::Result<()> {
    let agent = ssh::agent::Agent::connect()?;

    term::headline(&format!(
        "🌱 Authenticating as {}",
        term::format::Identity::new(profile).styled()
    ));

    let profile = &profile;
    if !agent.signer(profile.public_key).is_ready()? {
        term::warning("Adding your radicle key to ssh-agent...");

        // TODO: We should show the spinner on the passphrase prompt,
        // otherwise it seems like the passphrase is valid even if it isn't.
        let passphrase = term::read_passphrase(options.stdin, false)?;
        let spinner = term::spinner("Unlocking...");
        let mut agent = ssh::agent::Agent::connect()?;
        let secret = profile
            .keystore
            .secret_key(passphrase)?
            .ok_or_else(|| anyhow!("Key not found in {:?}", profile.keystore.path()))?;
        agent.register(&secret)?;
        spinner.finish();

        term::success!("Radicle key added to ssh-agent");
    } else {
        term::success!("Signing key already in ssh-agent");
    };

    Ok(())
}

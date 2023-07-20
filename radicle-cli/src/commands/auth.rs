#![allow(clippy::or_fun_call)]
use std::env;
use std::ffi::OsString;
use std::ops::Not as _;
use std::str::FromStr;

use anyhow::anyhow;

use radicle::crypto::ssh;
use radicle::crypto::ssh::Passphrase;
use radicle::node::Alias;
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

    --alias                 When initializing an identity, sets the node alias
    --stdin                 Read passphrase from stdin (default: false)
    --help                  Print help
"#,
};

#[derive(Debug)]
pub struct Options {
    pub stdin: bool,
    pub alias: Option<Alias>,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut stdin = false;
        let mut alias = None;
        let mut parser = lexopt::Parser::from_args(args);

        while let Some(arg) = parser.next()? {
            match arg {
                Long("alias") => {
                    let val = parser.value()?;
                    let val = term::args::alias(&val)?;

                    alias = Some(val);
                }
                Long("stdin") => {
                    stdin = true;
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((Options { alias, stdin }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    match ctx.profile() {
        Ok(profile) => authenticate(options, &profile),
        Err(_) => init(options),
    }
}

pub fn init(options: Options) -> anyhow::Result<()> {
    term::headline("Initializing your radicle 👾 identity");

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

    let alias: Alias = if let Some(alias) = options.alias {
        alias
    } else {
        let user = env::var("USER").ok().and_then(|u| Alias::from_str(&u).ok());
        term::input(
            "Enter your alias:",
            user,
            Some("This is your node alias. You can always change it later"),
        )?
    };
    let home = profile::home()?;
    let passphrase = if options.stdin {
        term::passphrase_stdin()
    } else {
        term::passphrase_confirm("Enter a passphrase:", RAD_PASSPHRASE)
    }?;
    let passphrase = passphrase.trim().is_empty().not().then_some(passphrase);
    let spinner = term::spinner("Creating your Ed25519 keypair...");
    let profile = Profile::init(home, alias, passphrase.clone())?;
    spinner.finish();

    if let Some(passphrase) = passphrase {
        match ssh::agent::Agent::connect() {
            Ok(mut agent) => {
                let mut spinner = term::spinner("Adding your radicle key to ssh-agent...");
                if register(&mut agent, &profile, passphrase).is_ok() {
                    spinner.finish();
                } else {
                    spinner.message("Could not register radicle key in ssh-agent.");
                    spinner.warn();
                }
            }
            Err(e) if e.is_not_running() => {}
            Err(e) => Err(e)?,
        }
    }

    term::success!(
        "Your Radicle DID is {}. This identifies your device.",
        term::format::highlight(profile.did())
    );

    term::blank();
    term::info!(
        "To create a radicle project, run {} from a git repository.",
        term::format::command("rad init")
    );

    Ok(())
}

/// Try loading the identity's key into SSH Agent, falling back to verifying `RAD_PASSPHRASE` for
/// use.
pub fn authenticate(options: Options, profile: &Profile) -> anyhow::Result<()> {
    if !profile.keystore.is_encrypted()? {
        term::success!("Authenticated as {}", term::format::tertiary(profile.id()));
        return Ok(());
    }

    // If our key is encrypted, we try to authenticate with SSH Agent and
    // register it; only if it is running.
    match ssh::agent::Agent::connect() {
        Ok(mut agent) => {
            if agent.request_identities()?.contains(&profile.public_key) {
                term::success!("Radicle key already in ssh-agent");
                return Ok(());
            }

            term::headline(format!(
                "Authenticating as 👾 {}",
                term::format::Identity::new(profile).styled()
            ));

            let passphrase = if options.stdin {
                term::passphrase_stdin()
            } else {
                term::passphrase(RAD_PASSPHRASE)
            }?;
            register(&mut agent, profile, passphrase)?;

            term::success!("Radicle key added to {}", term::format::dim("ssh-agent"));

            return Ok(());
        }
        Err(e) if e.is_not_running() => {}
        Err(e) => Err(e)?,
    };

    // Try RAD_PASSPHRASE fallback.
    if let Some(passphrase) = profile::env::passphrase() {
        ssh::keystore::MemorySigner::load(&profile.keystore, Some(passphrase))
            .map_err(|_| anyhow!("RAD_PASSPHRASE failed"))?;
        return Ok(());
    };

    // ssh-agent is the de-facto solution.
    anyhow::bail!("ssh-agent not running");
}

/// Register key with ssh-agent.
pub fn register(
    agent: &mut ssh::agent::Agent,
    profile: &Profile,
    passphrase: Passphrase,
) -> anyhow::Result<()> {
    let secret = profile
        .keystore
        .secret_key(Some(passphrase))
        .map_err(|e| {
            if e.is_crypto_err() {
                anyhow!("could not decrypt secret key: invalid passphrase")
            } else {
                e.into()
            }
        })?
        .ok_or_else(|| anyhow!("Key not found in {:?}", profile.keystore.path()))?;

    agent.register(&secret)?;

    Ok(())
}

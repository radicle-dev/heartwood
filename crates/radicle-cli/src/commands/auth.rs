#![allow(clippy::or_fun_call)]
mod args;

use std::str::FromStr;

use anyhow::{anyhow, Context};

use radicle::crypto::ssh;
use radicle::crypto::ssh::Passphrase;
use radicle::node::Alias;
use radicle::profile::env;
use radicle::{profile, Profile};

use crate::terminal as term;

pub use args::Args;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    match ctx.profile() {
        Ok(profile) => authenticate(args, &profile),
        Err(_) => init(args),
    }
}

pub fn init(args: Args) -> anyhow::Result<()> {
    term::headline("Initializing your radicle 👾 identity");

    if let Ok(version) = radicle::git::version() {
        if version < radicle::git::VERSION_REQUIRED {
            term::warning(format!(
                "Your Git version is unsupported, please upgrade to {} or later",
                radicle::git::VERSION_REQUIRED,
            ));
            term::blank();
        }
    } else {
        anyhow::bail!("A Git installation is required for Radicle to run.");
    }

    let alias: Alias = if let Some(alias) = args.alias {
        alias
    } else {
        let user = env::var("USER").ok().and_then(|u| Alias::from_str(&u).ok());
        let user = term::input(
            "Enter your alias:",
            user,
            Some("This is your node alias. You can always change it later"),
        )?;

        user.ok_or_else(|| anyhow::anyhow!("An alias is required for Radicle to run."))?
    };
    let home = profile::home()?;
    let passphrase = if args.stdin {
        Some(term::passphrase_stdin()?)
    } else {
        term::passphrase_confirm("Enter a passphrase:", env::RAD_PASSPHRASE)?
    };
    let passphrase = passphrase.filter(|passphrase| !passphrase.trim().is_empty());
    let spinner = term::spinner("Creating your Ed25519 keypair...");
    let profile = Profile::init(home, alias, passphrase.clone(), env::seed())?;
    let mut agent = true;
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
            Err(e) if e.is_not_running() => {
                agent = false;
            }
            Err(e) => Err(e).context("failed to connect to ssh-agent")?,
        }
    }

    term::success!(
        "Your Radicle DID is {}. This identifies your device. Run {} to show it at all times.",
        term::format::highlight(profile.did()),
        term::format::command("rad self")
    );
    term::success!("You're all set.");
    term::blank();

    if profile.config.cli.hints && !agent {
        term::hint("install ssh-agent to have it fill in your passphrase for you when signing.");
        term::blank();
    }
    term::info!(
        "To create a Radicle repository, run {} from a Git repository with at least one commit.",
        term::format::command("rad init")
    );
    term::info!(
        "To clone a repository, run {}. For example, {} clones the Radicle 'heartwood' repository.",
        term::format::command("rad clone <rid>"),
        term::format::command("rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5")
    );
    term::info!(
        "To get a list of all commands, run {}.",
        term::format::command("rad"),
    );

    Ok(())
}

/// Try loading the identity's key into SSH Agent, falling back to verifying `RAD_PASSPHRASE` for
/// use.
pub fn authenticate(args: Args, profile: &Profile) -> anyhow::Result<()> {
    if !profile.keystore.is_encrypted()? {
        term::success!("Authenticated as {}", term::format::tertiary(profile.id()));
        return Ok(());
    }
    for (key, _) in &profile.config.node.extra {
        term::warning(format!(
            "unused or deprecated configuration attribute {key:?}"
        ));
    }

    // If our key is encrypted, we try to authenticate with SSH Agent and
    // register it; only if it is running.
    match ssh::agent::Agent::connect() {
        Ok(mut agent) => {
            if agent.request_identities()?.contains(&profile.public_key) {
                term::success!("Radicle key already in ssh-agent");
                return Ok(());
            }
            let passphrase = if let Some(phrase) = profile::env::passphrase() {
                phrase
            } else if args.stdin {
                term::passphrase_stdin()?
            } else if let Some(passphrase) =
                term::io::passphrase(term::io::PassphraseValidator::new(profile.keystore.clone()))?
            {
                passphrase
            } else {
                anyhow::bail!(
                    "A passphrase is required to read your Radicle key. Unable to continue."
                )
            };
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
            .map_err(|_| anyhow!("`{}` is invalid", env::RAD_PASSPHRASE))?;
        return Ok(());
    }

    term::print(term::format::dim(
        "Nothing to do, ssh-agent is not running.",
    ));
    term::print(term::format::dim(
        "You will be prompted for a passphrase when necessary.",
    ));

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
        .secret_key(Some(passphrase))
        .map_err(|e| {
            if e.is_crypto_err() {
                anyhow!("could not decrypt secret key: invalid passphrase")
            } else {
                e.into()
            }
        })?
        .ok_or_else(|| anyhow!("Key not found in {:?}", profile.keystore.secret_key_path()))?;

    agent.register(&secret)?;

    Ok(())
}

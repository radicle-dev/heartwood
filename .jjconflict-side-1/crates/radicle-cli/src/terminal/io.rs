use anyhow::anyhow;
use radicle::cob::Reaction;
use radicle::cob::issue::Issue;
use radicle::cob::thread::{Comment, CommentId};
use radicle::crypto::ssh::Keystore;
use radicle::crypto::ssh::keystore::MemorySigner;
use radicle::node::device::{BoxedDevice, Device};
use radicle::profile::Profile;
use radicle::profile::env::RAD_PASSPHRASE;

pub use radicle_term::io::*;
pub use radicle_term::spinner;

use inquire::validator;

/// Validates secret key passphrases.
#[derive(Clone)]
pub struct PassphraseValidator {
    keystore: Keystore,
}

impl PassphraseValidator {
    /// Create a new validator.
    #[must_use]
    pub fn new(keystore: Keystore) -> Self {
        Self { keystore }
    }
}

impl inquire::validator::StringValidator for PassphraseValidator {
    fn validate(
        &self,
        input: &str,
    ) -> Result<validator::Validation, inquire::error::CustomUserError> {
        let passphrase = Passphrase::from(input.to_owned());
        if self.keystore.is_valid_passphrase(&passphrase)? {
            Ok(validator::Validation::Valid)
        } else {
            Ok(validator::Validation::Invalid(
                validator::ErrorMessage::from("Invalid passphrase, please try again"),
            ))
        }
    }
}

/// Get the signer. First we try getting it from ssh-agent, otherwise we prompt the user,
/// if we're connected to a TTY.
pub fn signer(profile: &Profile) -> anyhow::Result<BoxedDevice> {
    match profile.signer() {
        Ok(signer) => return Ok(signer),
        Err(err) if !err.prompt_for_passphrase() => return Err(anyhow!(err)),
        Err(_) => {
            // The error returned is potentially recoverable by prompting
            // the user for the correct passphrase.
        }
    }

    let validator = PassphraseValidator::new(profile.keystore.clone());
    let passphrase = match passphrase(validator)? {
        Some(p) => p,
        None => {
            anyhow::bail!(
                "A passphrase is required to read your Radicle key. Unable to continue. Consider setting the environment variable `{RAD_PASSPHRASE}`.",
            )
        }
    };
    let spinner = spinner("Unsealing key...");
    let signer = MemorySigner::load(&profile.keystore, Some(passphrase))?;

    spinner.finish();

    Ok(Device::from(signer).boxed())
}

pub fn comment_select(issue: &Issue) -> anyhow::Result<(&CommentId, &Comment)> {
    let comments = issue.comments().collect::<Vec<_>>();
    let selection = Select::new(
        "Which comment do you want to react to?",
        (0..comments.len()).collect(),
    )
    .with_render_config(*CONFIG)
    .with_formatter(&|i| comments.get(i.index).unwrap().1.body().to_owned())
    .prompt()?;

    comments
        .get(selection)
        .copied()
        .ok_or(anyhow!("failed to perform comment selection"))
}

pub fn reaction_select() -> anyhow::Result<Reaction> {
    let emoji = Select::new(
        "With which emoji do you want to react?",
        vec!['🐙', '👾', '💯', '✨', '🙇', '🙅', '❤'],
    )
    .with_render_config(*CONFIG)
    .prompt()?;
    Ok(Reaction::new(emoji)?)
}

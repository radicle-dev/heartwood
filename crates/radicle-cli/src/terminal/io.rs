use anyhow::anyhow;
use radicle::cob::Reaction;
use radicle::cob::issue::Issue;
use radicle::cob::thread::{Comment, CommentId};
use radicle::crypto::SigningKey;
use radicle::crypto::ssh::Keystore;
use radicle::profile::env::RAD_PASSPHRASE;
use radicle::profile::{Profile, Signer, SignerError};

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

/// Get the signer. First we try getting it from ssh-agent; otherwise, we prompt the user,
/// if we're connected to a TTY.
pub fn signer(profile: &Profile) -> anyhow::Result<Signer> {
    let err = match profile.signer() {
        Ok(signer) => return Ok(signer),
        Err(err) => err,
    };

    match err {
        SignerError::LoadError(radicle::crypto::LoadError::InvalidPassphrase) => {
            super::warning(format!(
                "The passphrase for your Radicle key provided in the environment variable `{RAD_PASSPHRASE}` is invalid. Please try again."
            ));
        }
        SignerError::AgentConnection(err) => {
            super::warning(format!(
                "Failed to connect to ssh-agent: {err}. Falling back to passphrase prompt."
            ));
        }
        SignerError::Agent(radicle::crypto::ssh::agent::IntoSignerError::IdentityNotFound {
            identity,
        }) => {
            super::warning(format!(
                "The Radicle key for `{identity}` is not registered with ssh-agent. Please run `rad auth` to register it."
            ));
        }
        err @ SignerError::LoadError(_)
        | err @ SignerError::InvalidPublicKey(_)
        | err @ SignerError::Agent(_)
        | err @ SignerError::Keystore(_) => return Err(anyhow!(err)),
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
    let spinner = spinner("Unsealing key…");
    let signer = SigningKey::load(&profile.keystore, Some(passphrase))?;

    spinner.finish();

    Ok(Signer::Key(signer))
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

use anyhow::anyhow;
use radicle::cob::issue::Issue;
use radicle::cob::thread::{Comment, CommentId};
use radicle::cob::Reaction;
use radicle::crypto::ssh::keystore::MemorySigner;
use radicle::crypto::{ssh::Keystore, Signer};
use radicle::profile::env::RAD_PASSPHRASE;
use radicle::profile::Profile;

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
pub fn signer(profile: &Profile) -> anyhow::Result<Box<dyn Signer>> {
    if let Ok(signer) = profile.signer() {
        return Ok(signer);
    }
    let validator = PassphraseValidator::new(profile.keystore.clone());
    let passphrase = match passphrase(validator) {
        Ok(p) => p,
        Err(inquire::InquireError::NotTTY) => {
            return Err(anyhow::anyhow!(
                "running in non-interactive mode, please set `{RAD_PASSPHRASE}` to unseal your key",
            ));
        }
        Err(e) => return Err(e.into()),
    };
    let spinner = spinner("Unsealing key...");
    let signer = MemorySigner::load(&profile.keystore, Some(passphrase))?;

    spinner.finish();

    Ok(signer.boxed())
}

pub fn comment_select(issue: &Issue) -> anyhow::Result<(&CommentId, &Comment)> {
    let comments = issue.comments().collect::<Vec<_>>();
    let selection = Select::new(
        "Which comment do you want to react to?",
        (0..comments.len()).collect(),
    )
    .with_render_config(*CONFIG)
    .with_formatter(&|i| comments[i.index].1.body().to_owned())
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

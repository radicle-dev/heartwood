use radicle::cob::issue::Issue;
use radicle::cob::thread::{Comment, CommentId};
use radicle::crypto::ssh::keystore::MemorySigner;
use radicle::crypto::Signer;
use radicle::profile::env::RAD_PASSPHRASE;
use radicle::profile::Profile;

pub use radicle_term::io::*;
pub use radicle_term::spinner;

/// Get the signer. First we try getting it from ssh-agent, otherwise we prompt the user.
pub fn signer(profile: &Profile) -> anyhow::Result<Box<dyn Signer>> {
    if let Ok(signer) = profile.signer() {
        return Ok(signer);
    }
    let passphrase = passphrase(RAD_PASSPHRASE)?;
    let spinner = spinner("Unsealing key...");
    let signer = MemorySigner::load(&profile.keystore, passphrase)?;

    spinner.finish();

    Ok(signer.boxed())
}

pub fn comment_select(issue: &Issue) -> Option<(&CommentId, &Comment)> {
    let comments = issue.comments().collect::<Vec<_>>();
    let selection = Select::new(
        "Which comment do you want to react to?",
        (0..comments.len()).collect(),
    )
    .with_render_config(*CONFIG)
    .with_formatter(&|i| comments[i.index].1.body().to_owned())
    .prompt()
    .ok()?;

    comments.get(selection).copied()
}

pub mod proposal {
    use std::fmt::Write as _;

    use radicle::{
        cob::identity::{self, Proposal},
        git::Oid,
        identity::Identity,
    };

    use super::*;
    use crate::terminal::format;

    pub fn revision_select(
        proposal: &Proposal,
    ) -> Option<(&identity::RevisionId, &identity::Revision)> {
        let revisions = proposal.revisions().collect::<Vec<_>>();
        let selection = Select::new(
            "Which revision do you want to select?",
            (0..revisions.len()).collect(),
        )
        .with_vim_mode(true)
        .with_formatter(&|ix| revisions[ix.index].0.to_string())
        .with_render_config(*CONFIG)
        .prompt()
        .ok()?;

        revisions.get(selection).copied()
    }

    pub fn revision_commit_select<'a>(
        proposal: &'a Proposal,
        previous: &'a Identity<Oid>,
    ) -> Option<(&'a identity::RevisionId, &'a identity::Revision)> {
        let revisions = proposal
            .revisions()
            .filter(|(_, r)| r.is_quorum_reached(previous))
            .collect::<Vec<_>>();
        let selection = Select::new(
            "Which revision do you want to commit?",
            (0..revisions.len()).collect(),
        )
        .with_formatter(&|ix| revisions[ix.index].0.to_string())
        .with_render_config(*CONFIG)
        .prompt()
        .ok()?;

        revisions.get(selection).copied()
    }

    pub fn diff(proposal: &identity::Revision, previous: &Identity<Oid>) -> anyhow::Result<String> {
        use similar::{ChangeTag, TextDiff};

        let new = serde_json::to_string_pretty(&proposal.proposed)?;
        let previous = serde_json::to_string_pretty(&previous.doc)?;
        let diff = TextDiff::from_lines(&previous, &new);
        let mut buf = String::new();
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Delete => write!(buf, "{}", format::negative(format!("-{change}")))?,
                ChangeTag::Insert => write!(buf, "{}", format::positive(format!("+{change}")))?,
                ChangeTag::Equal => write!(buf, " {change}")?,
            };
        }

        Ok(buf)
    }
}

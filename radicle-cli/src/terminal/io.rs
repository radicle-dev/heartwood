use std::fmt;

use inquire::ui::{ErrorMessageRenderConfig, StyleSheet, Styled};
use inquire::InquireError;
use inquire::{ui::Color, ui::RenderConfig, Confirm, CustomType, Password, Select};
use once_cell::sync::Lazy;

use radicle::cob::issue::Issue;
use radicle::cob::thread::{Comment, CommentId};
use radicle::crypto::ssh::keystore::{MemorySigner, Passphrase};
use radicle::crypto::Signer;
use radicle::profile;
use radicle::profile::Profile;

use super::command;
use super::format;
use super::spinner::spinner;
use super::Error;
use super::{style, Paint};

pub const ERROR_PREFIX: Paint<&str> = Paint::red("✗");
pub const TAB: &str = "    ";

/// Render configuration.
pub static CONFIG: Lazy<RenderConfig> = Lazy::new(|| RenderConfig {
    prompt: StyleSheet::new().with_fg(Color::LightCyan),
    prompt_prefix: Styled::new("?").with_fg(Color::LightBlue),
    answered_prompt_prefix: Styled::new("✓").with_fg(Color::LightGreen),
    answer: StyleSheet::new(),
    highlighted_option_prefix: Styled::new("*").with_fg(Color::LightYellow),
    help_message: StyleSheet::new().with_fg(Color::DarkGrey),
    error_message: ErrorMessageRenderConfig::default_colored()
        .with_prefix(Styled::new("✗").with_fg(Color::LightRed)),
    ..RenderConfig::default_colored()
});

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => ({
        println!("{}", format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => ({
        $crate::terminal::io::success_args(format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! tip {
    ($($arg:tt)*) => ({
        $crate::terminal::io::tip_args(format_args!($($arg)*));
    })
}

pub use info;
pub use success;
pub use tip;

pub fn success_args(args: fmt::Arguments) {
    println!("{} {args}", Paint::green("✓"));
}

pub fn tip_args(args: fmt::Arguments) {
    println!("👉 {}", style(format!("{args}")).italic());
}

pub fn columns() -> Option<usize> {
    termion::terminal_size().map(|(cols, _)| cols as usize).ok()
}

pub fn headline(headline: &str) {
    println!();
    println!("{}", style(headline).bold());
    println!();
}

pub fn header(header: &str) {
    println!();
    println!("{}", style(format::yellow(header)).bold().underline());
    println!();
}

pub fn blob(text: impl fmt::Display) {
    println!("{}", style(text.to_string().trim()).dim());
}

pub fn blank() {
    println!()
}

pub fn print(msg: impl fmt::Display) {
    println!("{msg}");
}

pub fn prefixed(prefix: &str, text: &str) -> String {
    text.split('\n')
        .map(|line| format!("{prefix}{line}\n"))
        .collect()
}

pub fn help(name: &str, version: &str, description: &str, usage: &str) {
    println!("rad-{name} {version}\n{description}\n{usage}");
}

pub fn usage(name: &str, usage: &str) {
    println!(
        "{} {}\n{}",
        ERROR_PREFIX,
        Paint::red(format!("Error: rad-{name}: invalid usage")),
        Paint::red(prefixed(TAB, usage)).dim()
    );
}

pub fn println(prefix: impl fmt::Display, msg: impl fmt::Display) {
    println!("{prefix} {msg}");
}

pub fn indented(msg: impl fmt::Display) {
    println!("{TAB}{msg}");
}

pub fn subcommand(msg: impl fmt::Display) {
    println!("{} {}", style("$").dim(), style(msg).dim());
}

pub fn warning(warning: &str) {
    println!(
        "{} {} {warning}",
        Paint::yellow("!"),
        Paint::yellow("Warning:").bold(),
    );
}

pub fn error(error: impl fmt::Display) {
    println!("{ERROR_PREFIX} {error}");
}

pub fn fail(header: &str, error: &anyhow::Error) {
    let err = error.to_string();
    let err = err.trim_end();
    let separator = if err.contains('\n') { ":\n" } else { ": " };

    println!(
        "{ERROR_PREFIX} {}{}{error}",
        Paint::red(header).bold(),
        Paint::red(separator),
    );

    if let Some(Error::WithHint { hint, .. }) = error.downcast_ref::<Error>() {
        println!("{} {}", Paint::yellow("×"), Paint::yellow(hint));
        blank();
    }
}

pub fn ask<D: fmt::Display>(prompt: D, default: bool) -> bool {
    let prompt = format!("{} {}", Paint::blue("?".to_owned()), prompt);

    Confirm::new(&prompt)
        .with_default(default)
        .prompt()
        .unwrap_or_default()
}

pub fn confirm<D: fmt::Display>(prompt: D) -> bool {
    ask(format::tertiary(prompt), true)
}

pub fn abort<D: fmt::Display>(prompt: D) -> bool {
    ask(format::tertiary(prompt), false)
}

/// Get the signer. First we try getting it from ssh-agent, otherwise we prompt the user.
pub fn signer(profile: &Profile) -> anyhow::Result<Box<dyn Signer>> {
    if let Ok(signer) = profile.signer() {
        return Ok(signer);
    }
    let passphrase = passphrase()?;
    let spinner = spinner("Unsealing key...");
    let signer = MemorySigner::load(&profile.keystore, passphrase)?;

    spinner.finish();

    Ok(signer.boxed())
}

pub fn input<S, E>(message: &str, default: Option<S>) -> anyhow::Result<S>
where
    S: fmt::Display + std::str::FromStr<Err = E> + Clone,
    E: fmt::Debug + fmt::Display,
{
    let input = CustomType::<S>::new(message).with_render_config(*CONFIG);
    let value = match default {
        Some(default) => input.with_default(default).prompt()?,
        None => input.prompt()?,
    };
    Ok(value)
}

pub fn passphrase() -> Result<Passphrase, anyhow::Error> {
    if let Some(p) = profile::env::passphrase() {
        Ok(p)
    } else {
        Ok(Passphrase::from(
            Password::new("Passphrase:")
                .with_render_config(*CONFIG)
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .without_confirmation()
                .prompt()?,
        ))
    }
}

pub fn passphrase_confirm() -> Result<Passphrase, anyhow::Error> {
    if let Some(p) = profile::env::passphrase() {
        Ok(p)
    } else {
        Ok(Passphrase::from(
            Password::new("Passphrase:")
                .with_render_config(*CONFIG)
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .with_custom_confirmation_message("Repeat passphrase:")
                .with_custom_confirmation_error_message("The passphrases don't match.")
                .prompt()?,
        ))
    }
}

pub fn passphrase_stdin() -> Result<Passphrase, anyhow::Error> {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    Ok(Passphrase::from(input.trim_end().to_owned()))
}

pub fn select<'a, T>(
    prompt: &str,
    options: &'a [T],
    active: &'a T,
) -> Result<Option<&'a T>, InquireError>
where
    T: fmt::Display + Eq + PartialEq,
{
    let active = options.iter().position(|o| o == active);
    let selection =
        Select::new(prompt, options.iter().collect::<Vec<_>>()).with_render_config(*CONFIG);

    if let Some(active) = active {
        selection.with_starting_cursor(active).prompt_skippable()
    } else {
        selection.prompt_skippable()
    }
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

pub fn markdown(content: &str) {
    if !content.is_empty() && command::bat(["-p", "-l", "md"], content).is_err() {
        blob(content);
    }
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

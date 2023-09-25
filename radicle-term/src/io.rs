use std::ffi::OsStr;
use std::{env, fmt, io, process};

use inquire::ui::{ErrorMessageRenderConfig, StyleSheet, Styled};
use inquire::validator;
use inquire::InquireError;
use inquire::{ui::Color, ui::RenderConfig, Confirm, CustomType, Password};
use once_cell::sync::Lazy;
use zeroize::Zeroizing;

use crate::command;
use crate::format;
use crate::{style, Paint, Size};

pub use inquire;
pub use inquire::Select;

pub const ERROR_PREFIX: Paint<&str> = Paint::red("✗");
pub const ERROR_HINT_PREFIX: Paint<&str> = Paint::yellow("✗ Hint:");
pub const WARNING_PREFIX: Paint<&str> = Paint::yellow("!");
pub const TAB: &str = "    ";

/// Passphrase input.
pub type Passphrase = Zeroizing<String>;

/// Render configuration.
pub static CONFIG: Lazy<RenderConfig> = Lazy::new(|| RenderConfig {
    prompt: StyleSheet::new().with_fg(Color::LightCyan),
    prompt_prefix: Styled::new("?").with_fg(Color::LightBlue),
    answered_prompt_prefix: Styled::new("✓").with_fg(Color::LightGreen),
    answer: StyleSheet::new(),
    highlighted_option_prefix: Styled::new("✓").with_fg(Color::LightYellow),
    selected_option: Some(StyleSheet::new().with_fg(Color::LightYellow)),
    option: StyleSheet::new(),
    help_message: StyleSheet::new().with_fg(Color::DarkGrey),
    default_value: StyleSheet::new().with_fg(Color::LightBlue),
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
        $crate::io::success_args(format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! tip {
    ($($arg:tt)*) => ({
        $crate::io::tip_args(format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! notice {
    ($($arg:tt)*) => ({
        $crate::io::notice_args(format_args!($($arg)*));
    })
}

pub use info;
pub use notice;
pub use success;
pub use tip;

pub fn success_args(args: fmt::Arguments) {
    println!("{} {args}", Paint::green("✓"));
}

pub fn tip_args(args: fmt::Arguments) {
    println!(
        "{} {}",
        format::yellow("*"),
        style(format!("{args}")).italic()
    );
}

pub fn notice_args(args: fmt::Arguments) {
    println!("{} {args}", Paint::new("!").dim());
}

pub fn columns() -> Option<usize> {
    termion::terminal_size().map(|(cols, _)| cols as usize).ok()
}

pub fn viewport() -> Option<Size> {
    termion::terminal_size()
        .map(|(cols, rows)| Size::new(cols as usize, rows as usize))
        .ok()
}

pub fn headline(headline: impl fmt::Display) {
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

pub fn manual(name: &str) -> io::Result<process::ExitStatus> {
    let mut child = process::Command::new("man")
        .arg(format!("rad-{name}"))
        .spawn()?;

    child.wait()
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
    println!("{}", style(format!("Running `{msg}`...")).dim());
}

pub fn warning(warning: impl fmt::Display) {
    println!(
        "{} {} {warning}",
        WARNING_PREFIX,
        Paint::yellow("Warning:").bold(),
    );
}

pub fn error(error: impl fmt::Display) {
    println!("{ERROR_PREFIX} {error}");
}

pub fn ask<D: fmt::Display>(prompt: D, default: bool) -> bool {
    let prompt = prompt.to_string();

    Confirm::new(&prompt)
        .with_default(default)
        .with_render_config(*CONFIG)
        .prompt()
        .unwrap_or_default()
}

pub fn confirm<D: fmt::Display>(prompt: D) -> bool {
    ask(prompt, true)
}

pub fn abort<D: fmt::Display>(prompt: D) -> bool {
    ask(prompt, false)
}

pub fn input<S, E>(message: &str, default: Option<S>, help: Option<&str>) -> anyhow::Result<S>
where
    S: fmt::Display + std::str::FromStr<Err = E> + Clone,
    E: fmt::Debug + fmt::Display,
{
    let mut input = CustomType::<S>::new(message).with_render_config(*CONFIG);

    input.default = default;
    input.help_message = help;

    let value = input.prompt()?;

    Ok(value)
}

pub fn passphrase<K: AsRef<OsStr>, V: validator::StringValidator + 'static>(
    var: K,
    validate: V,
) -> Result<Passphrase, inquire::InquireError> {
    if let Ok(p) = env::var(var) {
        Ok(Passphrase::from(p))
    } else {
        Ok(Passphrase::from(
            Password::new("Passphrase:")
                .with_render_config(*CONFIG)
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .without_confirmation()
                .with_validator(validate)
                .prompt()?,
        ))
    }
}

pub fn passphrase_confirm<K: AsRef<OsStr>>(
    prompt: &str,
    var: K,
) -> Result<Passphrase, anyhow::Error> {
    if let Ok(p) = env::var(var) {
        Ok(Passphrase::from(p))
    } else {
        Ok(Passphrase::from(
            Password::new(prompt)
                .with_render_config(*CONFIG)
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .with_custom_confirmation_message("Repeat passphrase:")
                .with_custom_confirmation_error_message("The passphrases don't match.")
                .with_help_message("Leave this blank to keep your radicle key unencrypted")
                .prompt()?,
        ))
    }
}

pub fn passphrase_stdin() -> Result<Passphrase, anyhow::Error> {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    Ok(Passphrase::from(input.trim_end().to_owned()))
}

pub fn select<'a, T>(prompt: &str, options: &'a [T], help: &str) -> Result<&'a T, InquireError>
where
    T: fmt::Display + Eq + PartialEq,
{
    let selection = Select::new(prompt, options.iter().collect::<Vec<_>>())
        .with_vim_mode(true)
        .with_help_message(help)
        .with_render_config(*CONFIG);

    selection.with_starting_cursor(0).prompt()
}

pub fn markdown(content: &str) {
    if !content.is_empty() && command::bat(["-p", "-l", "md"], content).is_err() {
        blob(content);
    }
}

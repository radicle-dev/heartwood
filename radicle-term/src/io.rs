use std::ffi::OsStr;
use std::fmt::Write;
use std::process::Stdio;
use std::{env, fmt, io, process};

use inquire::ui::{ErrorMessageRenderConfig, StyleSheet, Styled};
use inquire::validator;
use inquire::InquireError;
use inquire::{ui::Color, ui::RenderConfig, Confirm, CustomType, Password};
use once_cell::sync::Lazy;
use zeroize::Zeroizing;

use crate::command;
use crate::format;
use crate::{display, style, Display, Paint, Size};

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
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.info(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
    ($writer:expr; $($arg:tt)*) => ({
        writeln!($writer, $($arg)*).ok();
    });
}

#[macro_export]
macro_rules! success {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.success(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

#[macro_export]
macro_rules! tip {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.tip(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

#[macro_export]
macro_rules! notice {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.notice(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

#[macro_export]
macro_rules! error {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.error(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

#[macro_export]
macro_rules! warning {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.warning(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

#[macro_export]
macro_rules! hint {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.hint(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

#[macro_export]
macro_rules! println {
    ($term:expr, $pattern:literal $(,$arg:expr)* $(,)?) => ({
        term.println(std::fmt::format(std::format_args!($pattern $(, term.display(&$arg))*)));
    });
}

pub use error;
pub use hint;
pub use info;
pub use println;
pub use notice;
pub use success;
pub use tip;
pub use warning;

#[deprecated]
pub fn columns() -> Option<usize> {
    termion::terminal_size().map(|(cols, _)| cols as usize).ok()
}

#[deprecated]
pub fn rows() -> Option<usize> {
    termion::terminal_size().map(|(_, rows)| rows as usize).ok()
}

#[deprecated]
pub fn viewport() -> Option<Size> {
    termion::terminal_size()
        .map(|(cols, rows)| Size::new(cols as usize, rows as usize))
        .ok()
}

#[deprecated]
pub fn headline(headline: impl fmt::Display) {
    std::println!();
    std::println!("{}", display(&style(headline).bold()));
    std::println!();
}

#[deprecated]
pub fn header(header: &str) {
    std::println!();
    std::println!("{}", display(&format::yellow(header).bold().underline()));
    std::println!();
}

#[deprecated]
pub fn blob(text: impl fmt::Display) {
    std::println!("{}", display(&style(text.to_string().trim()).dim()));
}

#[deprecated]
pub fn blank() {
    std::println!()
}

#[deprecated]
pub fn print(msg: impl fmt::Display) {
    std::println!("{msg}");
}

#[deprecated]
pub fn print_display(msg: &impl Display) {
    std::println!("{}", display(msg));
}

#[deprecated]
pub fn prefixed(prefix: &str, text: &str) -> String {
    text.split('\n').fold(String::new(), |mut s, line| {
        writeln!(&mut s, "{prefix}{line}").ok();
        s
    })
}

#[deprecated]
pub fn help(name: &str, version: &str, description: &str, usage: &str) {
    std::println!("rad-{name} {version}\n{description}\n{usage}");
}

#[deprecated]
pub fn manual(name: &str) -> io::Result<process::ExitStatus> {
    let mut child = process::Command::new("man")
        .arg(name)
        .stderr(Stdio::null())
        .spawn()?;

    child.wait()
}

#[deprecated]
pub fn usage(name: &str, usage: &str) {
    std::println!(
        "{} {}\n{}",
        display(&ERROR_PREFIX),
        display(&Paint::red(format!("Error: rad-{name}: invalid usage"))),
        display(&Paint::red(prefixed(TAB, usage)).dim())
    );
}

#[deprecated]
pub fn indented(msg: impl fmt::Display) {
    std::println!("{TAB}{msg}");
}

#[deprecated]
pub fn indented_display(msg: &impl Display) {
    std::println!("{TAB}{}", display(msg));
}

#[deprecated]
pub fn subcommand(msg: impl fmt::Display) {
    std::println!("{}", display(&style(format!("Running `{msg}`...")).dim()));
}

#[deprecated]
pub fn ask<D: fmt::Display>(prompt: D, default: bool) -> bool {
    let prompt = prompt.to_string();

    Confirm::new(&prompt)
        .with_default(default)
        .with_render_config(*CONFIG)
        .prompt()
        .unwrap_or_default()
}

#[deprecated]
pub fn confirm<D: fmt::Display>(prompt: D) -> bool {
    ask(prompt, true)
}

#[deprecated]
pub fn abort<D: fmt::Display>(prompt: D) -> bool {
    ask(prompt, false)
}

#[deprecated]
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

#[deprecated]
pub fn passphrase<V: validator::StringValidator + 'static>(
    validate: V,
) -> Result<Passphrase, inquire::InquireError> {
    Ok(Passphrase::from(
        Password::new("Passphrase:")
            .with_render_config(*CONFIG)
            .with_display_mode(inquire::PasswordDisplayMode::Masked)
            .without_confirmation()
            .with_validator(validate)
            .prompt()?,
    ))
}

#[deprecated]
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

#[deprecated]
pub fn passphrase_stdin() -> Result<Passphrase, anyhow::Error> {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    Ok(Passphrase::from(input.trim_end().to_owned()))
}

#[deprecated]
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

#[deprecated]
pub fn markdown(content: &str) {
    if !content.is_empty() && command::bat(["-p", "-l", "md"], content).is_err() {
        blob(content);
    }
}

#[cfg(test)]
mod test {
    use crate::{display, style, Display, Paint, Size};

    #[test]
    fn foo() {
        let term: crate::Terminal = Default::default();
        super::info!(term, "{} {}", "abc", &Paint::red("def"));
    }
}
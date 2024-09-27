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
use crate::{display, style, Context, Paint, Size, Display};

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
    ($writer:expr; $($arg:tt)*) => ({
        writeln!($writer, $($arg)*).ok();
    });
    ($($arg:tt)*) => ({
        println!("{}", format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! success {
    // Pattern when a writer is provided.
    ($writer:expr; $($arg:tt)*) => ({
        $crate::io::success_args($writer, format_args!($($arg)*));
    });
    // Pattern without writer.
    ($($arg:tt)*) => ({
        $crate::io::success_args(&mut std::io::stdout(), format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! tip {
    ($($arg:tt)*) => ({
        $crate::io::tip_args(format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! notice {
    // Pattern when a writer is provided.
    ($writer:expr; $($arg:tt)*) => ({
        $crate::io::notice_args($writer, format_args!($($arg)*));
    });
    ($($arg:tt)*) => ({
        $crate::io::notice_args(&mut std::io::stdout(), format_args!($($arg)*));
    })
}

pub use info;
pub use notice;
pub use success;
pub use tip;

pub fn success_args<W: io::Write>(w: &mut W, args: fmt::Arguments) {
    writeln!(w, "{} {args}", display(&Paint::green("✓"))).ok();
}

pub fn tip_args(args: fmt::Arguments) {
    println!(
        "{} {}",
        display(&format::yellow("*")),
        display(&style(format!("{args}")).italic())
    );
}

pub fn notice_args<W: io::Write>(w: &mut W, args: fmt::Arguments) {
    writeln!(w, "{} {args}", display(&Paint::new("!").dim())).ok();
}

pub fn columns() -> Option<usize> {
    termion::terminal_size().map(|(cols, _)| cols as usize).ok()
}

pub fn rows() -> Option<usize> {
    termion::terminal_size().map(|(_, rows)| rows as usize).ok()
}

pub fn viewport() -> Option<Size> {
    termion::terminal_size()
        .map(|(cols, rows)| Size::new(cols as usize, rows as usize))
        .ok()
}

pub fn headline(headline: impl fmt::Display) {
    println!();
    println!("{}", display(&style(headline).bold()));
    println!();
}

pub fn header(header: &str) {
    println!();
    println!("{}", display(&format::yellow(header).bold().underline()));
    println!();
}

pub fn blob(text: impl fmt::Display) {
    println!("{}", display(&style(text.to_string().trim()).dim()));
}

pub fn blank() {
    println!()
}

pub fn print(msg: impl fmt::Display) {
    println!("{msg}");
}

pub fn print_display<'a>(msg: &'a impl Display<'a>) {
    println!("{}", display(msg));
}

pub fn prefixed(prefix: &str, text: &str) -> String {
    text.split('\n').fold(String::new(), |mut s, line| {
        writeln!(&mut s, "{prefix}{line}").ok();
        s
    })
}

pub fn help(name: &str, version: &str, description: &str, usage: &str) {
    println!("rad-{name} {version}\n{description}\n{usage}");
}

pub fn manual(name: &str) -> io::Result<process::ExitStatus> {
    let mut child = process::Command::new("man")
        .arg(name)
        .stderr(Stdio::null())
        .spawn()?;

    child.wait()
}

pub fn usage(name: &str, usage: &str, context: &Context) {
    println!(
        "{} {}\n{}",
        ERROR_PREFIX.display(context),
        Paint::red(format!("Error: rad-{name}: invalid usage")).display(context),
        Paint::red(prefixed(TAB, usage)).dim().display(context)
    );
}

pub fn println(prefix: impl fmt::Display, msg: impl fmt::Display) {
    println!("{prefix} {msg}");
}

pub fn indented(msg: impl fmt::Display) {
    println!("{TAB}{msg}");
}

pub fn indented_display<'a>(msg: &'a impl Display<'a>) {
    println!("{TAB}{}", display(msg));
}

/*
pub fn subcommand(msg: impl fmt::Display) {
    println!("{}", style(format!("Running `{msg}`...")).dim());
}
*/

pub fn warning(warning: impl fmt::Display) {
    println!(
        "{} {} {warning}",
        display(&WARNING_PREFIX),
        display(&Paint::yellow("Warning:").bold()),
    );
}

pub fn error(error: impl fmt::Display) {
    println!("{} {} {error}", display(&ERROR_PREFIX), display(&Paint::red("Error:")));
}

pub fn hint(hint: impl fmt::Display) {
    println!("{} {}",display(&ERROR_HINT_PREFIX), display(&format::hint(hint)));
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

use std::ffi::OsStr;
use std::fmt::Write;
use std::process::Stdio;
use std::sync::LazyLock;
use std::{env, fmt, io, process};

use inquire::InquireError;
use inquire::ui::{ErrorMessageRenderConfig, StyleSheet, Styled};
use inquire::validator;
use inquire::{Confirm, CustomType, Password, ui::Color, ui::RenderConfig};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::format;
use crate::{Paint, Size, style};

pub use inquire;
pub use inquire::Select;

pub(crate) const SYMBOL_ERROR: &str = "✗";
pub(crate) const SYMBOL_SUCCESS: &str = "✓";
pub(crate) const SYMBOL_WARNING: &str = "!";

pub const PREFIX_ERROR: Paint<&str> = Paint::red(SYMBOL_ERROR);
pub const PREFIX_SUCCESS: Paint<&str> = Paint::green(SYMBOL_SUCCESS);
pub const PREFIX_WARNING: Paint<&str> = Paint::yellow(SYMBOL_WARNING);

pub const TAB: &str = "    ";

/// Passphrase input.
pub type Passphrase = Zeroizing<String>;

/// Render configuration.
pub static CONFIG: LazyLock<RenderConfig> = LazyLock::new(|| RenderConfig {
    prompt: StyleSheet::new().with_fg(Color::LightCyan),
    prompt_prefix: Styled::new("?").with_fg(Color::LightBlue),
    answered_prompt_prefix: Styled::new(SYMBOL_SUCCESS).with_fg(Color::LightGreen),
    answer: StyleSheet::new(),
    highlighted_option_prefix: Styled::new(SYMBOL_SUCCESS).with_fg(Color::LightYellow),
    selected_option: Some(StyleSheet::new().with_fg(Color::LightYellow)),
    option: StyleSheet::new(),
    help_message: StyleSheet::new().with_fg(Color::DarkGrey),
    default_value: StyleSheet::new().with_fg(Color::LightBlue),
    error_message: ErrorMessageRenderConfig::default_colored()
        .with_prefix(Styled::new(SYMBOL_ERROR).with_fg(Color::LightRed)),
    ..RenderConfig::default_colored()
});

/// Target for paint operations.
///
/// This tells a [`Spinner`] object where to paint to.
///
/// [`Spinner`]: crate::Spinner
#[derive(Clone)]
pub enum PaintTarget {
    Stdout,
    Stderr,
    Hidden,
}

impl PaintTarget {
    pub fn writer(&self) -> Box<dyn io::Write> {
        match self {
            PaintTarget::Stdout => Box::new(io::stdout()),
            PaintTarget::Stderr => Box::new(io::stderr()),
            PaintTarget::Hidden => Box::new(io::sink()),
        }
    }
}

#[macro_export]
macro_rules! info {
    ($writer:expr_2021; $($arg:tt)*) => ({
        writeln!($writer, $($arg)*).ok();
    });
    ($($arg:tt)*) => ({
        println!("{}", format_args!($($arg)*));
    })
}

#[macro_export]
macro_rules! success {
    // Pattern when a writer is provided.
    ($writer:expr_2021; $($arg:tt)*) => ({
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
    ($writer:expr_2021; $($arg:tt)*) => ({
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
    writeln!(w, "{PREFIX_SUCCESS} {args}").ok();
}

pub fn tip_args(args: fmt::Arguments) {
    println!(
        "{} {}",
        format::yellow("*"),
        style(format!("{args}")).italic()
    );
}

pub fn notice_args<W: io::Write>(w: &mut W, args: fmt::Arguments) {
    writeln!(w, "{} {args}", Paint::new(SYMBOL_WARNING).dim()).ok();
}

pub fn columns() -> Option<usize> {
    crossterm::terminal::size()
        .map(|(cols, _)| cols as usize)
        .ok()
}

pub fn rows() -> Option<usize> {
    crossterm::terminal::size()
        .map(|(_, rows)| rows as usize)
        .ok()
}

pub fn viewport() -> Option<Size> {
    crossterm::terminal::size()
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

pub fn usage(name: &str, usage: &str) {
    println!(
        "{} {}\n{}",
        PREFIX_ERROR,
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
        PREFIX_WARNING,
        Paint::yellow("Warning:").bold(),
    );
}

pub fn error(error: impl fmt::Display) {
    println!("{PREFIX_ERROR} {} {error}", Paint::red("Error:"));
}

pub fn hint(hint: impl fmt::Display) {
    println!("{}", format::hint(format!("{SYMBOL_ERROR} Hint: {hint}")));
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

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum InputError<Custom> {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Custom(Custom),
}

impl From<InputError<std::convert::Infallible>> for io::Error {
    fn from(val: InputError<std::convert::Infallible>) -> Self {
        match val {
            InputError::Io(err) => err,
            InputError::Custom(_) => unreachable!("infallible cannot be constructed"),
        }
    }
}

/// Prompts the user for input. If the user cancels the operation,
/// the operation is interrupted, or no suitable terminal is found,
/// then `Ok(None)` is returned.
pub fn input<T, E>(
    message: &str,
    default: Option<T>,
    help: Option<&str>,
) -> Result<Option<T>, InputError<E>>
where
    T: fmt::Display + std::str::FromStr<Err = E> + Clone,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut input = CustomType::<T>::new(message).with_render_config(*CONFIG);

    input.default = default;
    input.help_message = help;

    match input.prompt() {
        Ok(value) => Ok(Some(value)),
        Err(err) => handle_inquire_error(err),
    }
}

/// If the [`InquireError`] value is one of the variants:
/// [`InquireError::OperationCanceled`], [`InquireError::OperationInterrupted`],
/// [`InquireError::NotTTY`], then the returned result is `None` – note that no
/// `Some` value is returned.
///
/// Otherwise, the error is converted into our own domain error: [`InputError`].
fn handle_inquire_error<T, E>(error: InquireError) -> Result<Option<T>, InputError<E>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    use InquireError::*;

    let inner = match error {
        OperationCanceled | OperationInterrupted | NotTTY => None,
        InvalidConfiguration(err) => {
            // This case not reachable, as long as the configuration passed
            // to `prompt` is valid.
            // The configuration is *mostly* taken from `CONFIG`,
            // except for the added `CustomType` being prompted for.
            // We demand that these must not depend on user input in
            // a way that makes the configuration invalid.
            // If this is the case, `CONFIG` should be reassessed, or
            // the caller must control their input for the `CustomType`
            // better. In any case, such errors are not recoverable,
            // and certainly the user cannot do anything in that
            // situation. Their input should not affect the config,
            // that's the whole idea!
            panic!("{err}")
        }
        IO(err) => Some(InputError::Io(err)),
        Custom(err) => {
            match err.downcast::<E>() {
                Ok(err) => Some(InputError::Custom(*err)),
                Err(err) => {
                    // `inquire` guarantees that we do not end up here:
                    // https://github.com/mikaelmello/inquire/blob/4ac91f3e1fc8b29fc17845f9204ea1d1f9e335aa/README.md?plain=1#L109
                    panic!("inquire returned an unexpected error: {err:?}")
                }
            }
        }
    };

    match inner {
        Some(err) => Err(err),
        None => Ok(None),
    }
}

pub fn passphrase<V: validator::StringValidator + 'static>(
    validate: V,
) -> io::Result<Option<Passphrase>> {
    match Password::new("Passphrase:")
        .with_render_config(*CONFIG)
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .without_confirmation()
        .with_validator(validate)
        .prompt()
    {
        Ok(p) => Ok(Some(Passphrase::from(p))),
        Err(err) => handle_inquire_error(err).map_err(InputError::into),
    }
}

pub fn passphrase_confirm<K: AsRef<OsStr>>(prompt: &str, var: K) -> io::Result<Option<Passphrase>> {
    if let Ok(p) = env::var(var) {
        return Ok(Some(Passphrase::from(p)));
    }

    match Password::new(prompt)
        .with_render_config(*CONFIG)
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .with_custom_confirmation_message("Repeat passphrase:")
        .with_custom_confirmation_error_message("The passphrases don't match.")
        .with_help_message("Leave this blank to keep your Radicle key unencrypted")
        .prompt()
    {
        Ok(p) => Ok(Some(Passphrase::from(p))),
        Err(err) => handle_inquire_error(err).map_err(InputError::into),
    }
}

pub fn passphrase_stdin() -> io::Result<Passphrase> {
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

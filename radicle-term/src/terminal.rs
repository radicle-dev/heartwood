use crate::ansi::display::DisplayWrapper;
use crate::{command, format};
use crate::{display_with, style, Context, Display, Paint, Passphrase, Size};
use std::ffi::OsStr;
use std::{env, fmt, io};

use inquire::{
    ui::RenderConfig, validator::StringValidator, Confirm, CustomType, InquireError, Password,
    Select,
};

use crate::{CONFIG, ERROR_HINT_PREFIX, ERROR_PREFIX, TAB, WARNING_PREFIX};

pub struct Terminal<'a> {
    ctx: Context,
    config: RenderConfig<'a>,
}

impl Default for Terminal<'_> {
    fn default() -> Self {
        Self {
            ctx: Context::default(),
            config: *CONFIG,
        }
    }
}

impl Terminal<'_> {
    pub fn display<'a, T: Display<Context> + Sized + 'a>(
        &'a self,
        display: &'a T,
    ) -> impl fmt::Display + 'a {
        DisplayWrapper::new(display, &self.ctx)
    }

    pub fn info(&self, msg: impl fmt::Display) {
        // TODO
        println!(
            "{} {}",
            display_with(&Paint::cyan("ℹ"), &self.ctx),
            display_with(&msg, &self.ctx)
        )
    }

    pub fn success(&self, msg: impl Display) {
        println!(
            "{} {}",
            display_with(&Paint::green("✓"), &self.ctx),
            display_with(&msg, &self.ctx)
        )
    }

    pub fn tip_args(&self, args: fmt::Arguments) {
        println!(
            "{} {}",
            display_with(&format::yellow("*"), &self.ctx),
            display_with(&style(format!("{args}")).italic(), &self.ctx)
        );
    }

    fn notice_args<W: io::Write>(&self, w: &mut W, args: fmt::Arguments) {
        writeln!(
            w,
            "{} {args}",
            display_with(&Paint::new("!").dim(), &self.ctx)
        )
        .ok();
    }

    pub fn columns(&self) -> Option<usize> {
        termion::terminal_size().map(|(cols, _)| cols as usize).ok()
    }

    pub fn rows(&self) -> Option<usize> {
        termion::terminal_size().map(|(_, rows)| rows as usize).ok()
    }

    pub fn viewport(&self) -> Option<Size> {
        termion::terminal_size()
            .map(|(cols, rows)| Size::new(cols as usize, rows as usize))
            .ok()
    }

    pub fn headline(&self, headline: impl fmt::Display) {
        println!();
        println!("{}", display_with(&style(headline).bold(), &self.ctx));
        println!();
    }

    pub fn header(&self, header: &str) {
        println!();
        println!(
            "{}",
            display_with(&format::yellow(header).bold().underline(), &self.ctx)
        );
        println!();
    }

    pub fn blob(&self, text: impl fmt::Display) {
        println!(
            "{}",
            display_with(&style(text.to_string().trim()).dim(), &self.ctx)
        );
    }

    pub fn blank(&self) {
        println!()
    }

    pub fn print(&self, msg: &impl Display<Context>) {
        println!("{}", display_with(msg, &self.ctx))
    }

    pub fn prefixed(&self, prefix: &str, text: &str) -> String {
        use std::fmt::Write;

        text.split('\n').fold(String::new(), |mut s, line| {
            writeln!(&mut s, "{prefix}{line}").ok();
            s
        })
    }

    pub fn help(&self, name: &str, version: &str, description: &str, usage: &str) {
        println!("rad-{name} {version}\n{description}\n{usage}");
    }

    pub fn usage(&self, name: &str, usage: &str) {
        println!(
            "{} {}\n{}",
            display_with(&ERROR_PREFIX, &self.ctx),
            display_with(
                &Paint::red(format!("Error: rad-{name}: invalid usage")),
                &self.ctx
            ),
            display_with(&Paint::red(self.prefixed(TAB, usage)).dim(), &self.ctx)
        );
    }

    pub fn println(&self, prefix: impl fmt::Display, msg: impl fmt::Display) {
        println!("{prefix} {msg}");
    }

    pub fn indented(&self, msg: impl Display) {
        println!("{TAB}{}", self.display(&msg));
    }

    pub fn warning(&self, warning: impl fmt::Display) {
        println!(
            "{} {} {warning}",
            display_with(&WARNING_PREFIX, &self.ctx),
            display_with(&Paint::yellow("Warning:").bold(), &self.ctx),
        );
    }

    pub fn error(&self, error: impl fmt::Display) {
        println!(
            "{} {} {error}",
            display_with(&ERROR_PREFIX, &self.ctx),
            display_with(&Paint::red("Error:"), &self.ctx)
        );
    }

    pub fn hint(&self, hint: impl fmt::Display) {
        println!(
            "{} {}",
            display_with(&ERROR_HINT_PREFIX, &self.ctx),
            display_with(&format::hint(hint), &self.ctx)
        );
    }

    pub fn ask<D: fmt::Display>(&self, prompt: D, default: bool) -> bool {
        let prompt = prompt.to_string();

        Confirm::new(&prompt)
            .with_default(default)
            .with_render_config(self.config)
            .prompt()
            .unwrap_or_default()
    }

    pub fn confirm<D: fmt::Display>(&self, prompt: D) -> bool {
        self.ask(prompt, true)
    }

    pub fn abort<D: fmt::Display>(&self, prompt: D) -> bool {
        self.ask(prompt, false)
    }

    pub fn input<S, E>(
        &self,
        message: &str,
        default: Option<S>,
        help: Option<&str>,
    ) -> anyhow::Result<S>
    where
        S: fmt::Display + std::str::FromStr<Err = E> + Clone,
        E: fmt::Debug + fmt::Display,
    {
        let mut input = CustomType::<S>::new(message).with_render_config(self.config);

        input.default = default;
        input.help_message = help;

        let value = input.prompt()?;

        Ok(value)
    }

    pub fn passphrase<V: StringValidator + 'static>(
        &self,
        validate: V,
    ) -> Result<Passphrase, inquire::InquireError> {
        Ok(Passphrase::from(
            Password::new("Passphrase:")
                .with_render_config(self.config)
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .without_confirmation()
                .with_validator(validate)
                .prompt()?,
        ))
    }

    pub fn passphrase_confirm<K: AsRef<OsStr>>(
        &self,
        prompt: &str,
        var: K,
    ) -> Result<Passphrase, anyhow::Error> {
        if let Ok(p) = env::var(var) {
            Ok(Passphrase::from(p))
        } else {
            Ok(Passphrase::from(
                Password::new(prompt)
                    .with_render_config(self.config)
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

    pub fn select<'a, T>(
        &self,
        prompt: &str,
        options: &'a [T],
        help: &str,
    ) -> Result<&'a T, InquireError>
    where
        T: fmt::Display + Eq + PartialEq,
    {
        let selection = Select::new(prompt, options.iter().collect::<Vec<_>>())
            .with_vim_mode(true)
            .with_help_message(help)
            .with_render_config(self.config);

        selection.with_starting_cursor(0).prompt()
    }

    pub fn markdown(&self, content: &str) {
        if !content.is_empty() && command::bat(["-p", "-l", "md"], content).is_err() {
            self.blob(content);
        }
    }
}

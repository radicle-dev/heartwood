use crate::ansi::display::DisplayWrapper;
use crate::{command, format};
use crate::{style, Context, Display, Paint, Passphrase, Size};
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

    pub fn printlns<T>(&self, lines: Vec<T>)
    where
        T: Display,
    {
        for line in lines {
            self.println(line);
        }
    }

    pub fn println(&self, msg: impl Display) {
        println!("{}", self.display(&msg))
    }

    pub fn eprintln(&self, msg: impl Display) {
        eprintln!("{}", self.display(&msg))
    }

    pub fn info(&self, msg: impl Display) {
        println!("{} {}", self.display(&Paint::cyan("ℹ")), self.display(&msg))
    }

    pub fn success(&self, msg: impl Display) {
        println!(
            "{} {}",
            self.display(&Paint::green("✓")),
            self.display(&msg)
        )
    }

    pub fn tip(&self, msg: impl Display) {
        println!(
            "{} {}",
            self.display(&format::yellow("*")),
            self.display(&style(self.display(&msg).to_string()).italic())
        )
    }

    pub fn notice(&self, msg: impl Display) {
        println!(
            "{} {}",
            self.display(&Paint::new("!").dim()),
            self.display(&msg)
        )
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

    pub fn headline(&self, headline: impl Display) {
        println!();
        println!("{}", self.display(&style(headline).bold()));
        println!();
    }

    pub fn header(&self, header: &str) {
        println!();
        println!(
            "{}",
            self.display(&format::yellow(header).bold().underline())
        );
        println!();
    }

    pub fn blob(&self, text: impl Display) {
        println!(
            "{}",
            self.display(&style(self.display(&text).to_string().trim()).dim())
        );
    }

    pub fn blank(&self) {
        println!()
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
            self.display(&ERROR_PREFIX),
            self.display(&Paint::red(format!("Error: rad-{name}: invalid usage"))),
            self.display(&Paint::red(self.prefixed(TAB, usage)).dim()),
        );
    }

    pub fn indented(&self, msg: impl Display) {
        println!("{TAB}{}", self.display(&msg));
    }

    pub fn warning(&self, warning: impl Display) {
        println!(
            "{} {} {}",
            self.display(&WARNING_PREFIX),
            self.display(&Paint::yellow("Warning:").bold()),
            self.display(&warning)
        );
    }

    pub fn error(&self, error: impl Display) {
        println!(
            "{} {} {}",
            self.display(&ERROR_PREFIX),
            self.display(&Paint::red("Error:")),
            self.display(&error)
        );
    }

    pub fn hint(&self, hint: impl Display) {
        println!(
            "{} {}",
            self.display(&ERROR_HINT_PREFIX),
            self.display(&format::hint(self.display(&hint)))
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

    pub fn passphrase_stdin(&self) -> Result<Passphrase, anyhow::Error> {
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

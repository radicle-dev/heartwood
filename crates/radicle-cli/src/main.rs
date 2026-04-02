use std::ffi::OsString;
use std::fmt::Display;
use std::io;
use std::io::Write;
use std::{io::ErrorKind, process};

use anyhow::anyhow;
use clap::builder::Styles;
use clap::builder::styling::AnsiColor;
use clap::{CommandFactory as _, Parser, Subcommand};

use radicle::version::Version;
use radicle_cli::commands::*;
use radicle_cli::terminal as term;

pub const NAME: &str = "rad";
pub const GIT_HEAD: &str = env!("GIT_HEAD");
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RADICLE_VERSION: &str = env!("RADICLE_VERSION");
pub const RADICLE_VERSION_LONG: &str =
    concat!(env!("RADICLE_VERSION"), " (", env!("GIT_HEAD"), ")");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const LONG_DESCRIPTION: &str = "
Radicle is a sovereign code forge built on Git.

See `rad <COMMAND> --help` to learn about a specific command.

Do you have feedback?
 - Chat <\x1b]8;;https://radicle.zulipchat.com\x1b\\radicle.zulipchat.com\x1b]8;;\x1b\\>
 - Mail <\x1b]8;;mailto:feedback@radicle.dev\x1b\\feedback@radicle.dev\x1b]8;;\x1b\\>
   (Messages are automatically posted to the public #feedback channel on Zulip.)\
";
pub const TIMESTAMP: &str = env!("SOURCE_DATE_EPOCH");
pub const VERSION: Version = Version {
    name: NAME,
    version: RADICLE_VERSION,
    commit: GIT_HEAD,
    timestamp: TIMESTAMP,
};
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Magenta.on_default().bold())
    .usage(AnsiColor::Magenta.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());

/// Radicle command line interface
#[derive(Parser, Debug)]
#[command(name = NAME)]
#[command(version = RADICLE_VERSION)]
#[command(long_version = RADICLE_VERSION_LONG)]
#[command(about = DESCRIPTION)]
#[command(long_about = LONG_DESCRIPTION)]
#[command(propagate_version = true)]
#[command(styles = STYLES)]
struct CliArgs {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Auth(auth::Args),
    Block(block::Args),
    Checkout(checkout::Args),
    Clean(clean::Args),
    Clone(clone::Args),
    #[command(hide = true)]
    Cob(cob::Args),
    Config(config::Args),
    Debug(debug::Args),
    Follow(follow::Args),
    #[command(hide = true)] // `rad fork` command is deprecated
    Fork(fork::Args),
    Id(id::Args),
    Inbox(inbox::Args),
    Init(init::Args),
    #[command(alias = ".")]
    Inspect(inspect::Args),
    Issue(issue::Args),
    Ls(ls::Args),
    Node(node::Args),
    Patch(patch::Args),
    Path(path::Args),
    Publish(publish::Args),
    Remote(remote::Args),
    Seed(seed::Args),
    #[command(name = "self")]
    RadSelf(rad_self::Args),
    Stats(stats::Args),
    Sync(sync::Args),
    Unblock(unblock::Args),
    Unfollow(unfollow::Args),
    Unseed(unseed::Args),
    Watch(watch::Args),

    /// Print the version information of the CLI
    Version {
        /// Print the version information in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Print static completion information for a given shell
    #[command(hide = true)]
    Completion {
        /// The type of shell to output a static completion script for.
        shell: clap_complete::Shell,
    },

    #[command(external_subcommand)]
    External(Vec<OsString>),
}

fn main() {
    human_panic::setup_panic!(human_panic::Metadata::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )
    .homepage(env!("CARGO_PKG_HOMEPAGE"))
    .support("Open a support request at https://radicle.zulipchat.com/ or file an issue via Radicle itself, or e-mail to team@radicle.dev"));

    // Install a panic hook that intercepts panics caused by broken pipes and exits
    // cleanly. This is a backstop for any uses of `println!` (in our code or
    // dependencies like `clap`) that were not converted to `term::print`.
    //
    // `println!` panics with "failed printing to stdout: Broken pipe" when
    // failing to write to a closed standard output. We chain our hook in front
    // of `human_panic`'s hook so that panics not caused by broken pipes are
    // still handled by `human_panic`.
    //
    // See also <https://github.com/rust-lang/rust/issues/62569>.
    #[cfg(unix)]
    {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            handle_broken_pipe(info);
            default_hook(info);
        }));
    }

    if let Some(lvl) = radicle::logger::env_level() {
        let logger = Box::new(radicle::logger::Logger::new(lvl));
        log::set_boxed_logger(logger).expect("no other logger should have been set already");
        log::set_max_level(lvl.to_level_filter());
    }
    if let Err(e) = radicle::io::set_file_limit(4096) {
        log::warn!(target: "cli", "Unable to set open file limit: {e}");
    }
    let CliArgs { command } = CliArgs::parse();
    run(command, term::DefaultContext)
}

fn write_version(as_json: bool) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    if as_json {
        VERSION.write_json(&mut stdout)?;
        writeln!(&mut stdout)?;
        Ok(())
    } else {
        VERSION.write(&mut stdout)?;
        Ok(())
    }
}

fn run(command: Command, ctx: impl term::Context) -> ! {
    match run_command(command, ctx) {
        Ok(()) => process::exit(0),
        Err(err) => {
            // If the error is a broken pipe, exit cleanly. This happens when
            // output is piped to a command that exits before reading all our
            // output, e.g. `rad config | head`.
            //
            // Rust ignores `SIGPIPE` by default (since 1.62), so broken pipes
            // and instead returns `io::ErrorKind::BrokenPipe` errors on writes.
            // We want to catch these and exit cleanly.
            //
            // See <https://github.com/rust-lang/rust/issues/62569>.
            #[cfg(unix)]
            if is_broken_pipe(&err) {
                process::exit(0);
            }
            term::fail(&err);
            process::exit(1);
        }
    }
}

/// Handle an error of kind [`ErrorKind::BrokenPipe`] during a panic, and
/// exit the process with exit code 0.
///
/// # Debug
///
/// If compiled with `debug_assertions` enabled, then the panic is written to
/// [`std::io::stderr`].
#[cfg(unix)]
fn handle_broken_pipe(info: &std::panic::PanicHookInfo<'_>) {
    if !is_broken_pipe_panic(info) {
        return;
    }

    if cfg!(debug_assertions) {
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("<unnamed>");

        let mut stderr = std::io::stderr().lock();

        match info.location() {
            Some(location) => {
                let _ = writeln!(
                    stderr,
                    "broken pipe in thread '{thread}' at: {}:{}",
                    location.file(),
                    location.line(),
                );
            }
            None => {
                let _ = writeln!(stderr, "broken pipe in thread '{thread}'");
            }
        }

        #[cfg(feature = "backtrace")]
        let backtrace = format!("{:?}", backtrace::Backtrace::new());

        #[cfg(not(feature = "backtrace"))]
        let backtrace = "(no backtrace available)";

        let _ = writeln!(stderr, "{backtrace}");
    }
    process::exit(0);
}

/// Check if any error in the [`anyhow::Error::chain`] of `err` is of kind
/// [`ErrorKind::BrokenPipe`].
#[cfg(unix)]
fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|io_err| io_err.kind() == ErrorKind::BrokenPipe)
}

/// Check whether a panic was caused by writing to a broken pipe.
///
/// The standard library panics with a [`String`] payload containing
/// "Broken pipe" when [`println!`] or [`print!`] fail to write because standard
/// output is closed. This is stable behaviour across all Unix platforms, since
/// it is adopted from the description of `EPIPE` in [`errno.h` in POSIX.1-2024].
///
/// [`errno.h` in POSIX.1-2024]: https://pubs.opengroup.org/onlinepubs/9799919799.2024edition/basedefs/errno.h.html
#[cfg(unix)]
fn is_broken_pipe_panic(info: &std::panic::PanicHookInfo<'_>) -> bool {
    info.payload()
        .downcast_ref::<&'static str>()
        .copied()
        .or(info.payload().downcast_ref::<String>().map(|s| s.as_str()))
        .is_some_and(|message| message.contains("Broken pipe"))
}

fn run_command(command: Command, ctx: impl term::Context) -> Result<(), anyhow::Error> {
    match command {
        Command::Auth(args) => auth::run(args, ctx),
        Command::Block(args) => block::run(args, ctx),
        Command::Checkout(args) => checkout::run(args, ctx),
        Command::Clean(args) => clean::run(args, ctx),
        Command::Clone(args) => clone::run(args, ctx),
        Command::Cob(args) => cob::run(args, ctx),
        Command::Config(args) => config::run(args, ctx),
        Command::Debug(args) => debug::run(args, ctx),
        Command::Follow(args) => follow::run(args, ctx),
        Command::Fork(args) => fork::run(args, ctx),
        Command::Id(args) => id::run(args, ctx),
        Command::Inbox(args) => inbox::run(args, ctx),
        Command::Init(args) => init::run(args, ctx),
        Command::Inspect(args) => inspect::run(args, ctx),
        Command::Issue(args) => issue::run(args, ctx),
        Command::Ls(args) => ls::run(args, ctx),
        Command::Node(args) => node::run(args, ctx),
        Command::Patch(args) => patch::run(args, ctx),
        Command::Path(args) => path::run(args, ctx),
        Command::Publish(args) => publish::run(args, ctx),
        Command::Remote(args) => remote::run(args, ctx),
        Command::Seed(args) => seed::run(args, ctx),
        Command::RadSelf(args) => rad_self::run(args, ctx),
        Command::Stats(args) => stats::run(args, ctx),
        Command::Sync(args) => sync::run(args, ctx),
        Command::Unblock(args) => unblock::run(args, ctx),
        Command::Unfollow(args) => unfollow::run(args, ctx),
        Command::Unseed(args) => unseed::run(args, ctx),
        Command::Watch(args) => watch::run(args, ctx),
        Command::Version { json } => write_version(json),
        Command::Completion { shell } => {
            print_completion(shell, &mut CliArgs::command());
            Ok(())
        }
        Command::External(args) => ExternalCommand::new(args).run(),
    }
}

fn print_completion<G: clap_complete::Generator>(generator: G, cmd: &mut clap::Command) {
    clap_complete::generate(
        generator,
        cmd,
        cmd.get_name().to_string(),
        &mut io::stdout(),
    );
}

struct ExternalCommand {
    command: OsString,
    args: Vec<OsString>,
}

impl ExternalCommand {
    fn new(mut args: Vec<OsString>) -> Self {
        let command = args.remove(0);
        Self { command, args }
    }

    fn is_diff(&self) -> bool {
        self.command == "diff"
    }

    fn exe(&self) -> OsString {
        let mut exe = OsString::from(NAME);
        exe.push("-");
        exe.push(self.command.clone());
        exe
    }

    fn display_exe(&self) -> impl Display + use<> {
        match self.exe().into_string() {
            Ok(exe) => exe,
            Err(exe) => format!("{exe:?}"),
        }
    }

    fn run(self) -> anyhow::Result<()> {
        // This command is deprecated and delegates to `git diff`.
        // Even before it was deprecated, it was not printed by
        // `rad -h`.
        //
        // Since it is external, `--help` will delegate to `git diff --help`.
        if self.is_diff() {
            return diff::run(self.args);
        }

        let status = process::Command::new(self.exe()).args(&self.args).status();
        match status {
            Ok(status) => {
                if !status.success() {
                    return Err(anyhow!("`{}` exited with an error.", self.display_exe()));
                }
                Ok(())
            }
            Err(err) => {
                if let ErrorKind::NotFound = err.kind() {
                    Err(anyhow!(
                        "`{}` is not a known command. See `rad --help` for a list of commands.",
                        self.display_exe(),
                    ))
                } else {
                    Err(err.into())
                }
            }
        }
    }
}

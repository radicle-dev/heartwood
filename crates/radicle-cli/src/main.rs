use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::{io::ErrorKind, process};

use anyhow::anyhow;
use clap::builder::styling::AnsiColor;
use clap::builder::Styles;
use clap::{Parser, Subcommand};

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
pub const LONG_DESCRIPTION: &str = r#"
Radicle is a sovereign code forge built on Git.

See `rad <COMMAND> --help` to learn about a specific command.

Do you have feedback?

 - Chat <\x1b]8;;https://radicle.zulipchat.com\x1b\\radicle.zulipchat.com\x1b]8;;\x1b\\>
 - Mail <\x1b]8;;mailto:feedback@radicle.xyz\x1b\\feedback@radicle.xyz\x1b]8;;\x1b\\>
   (Messages are automatically posted to the public #feedback channel on Zulip.)
"#;
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

    #[command(external_subcommand)]
    External(Vec<OsString>),
}

fn main() {
    human_panic::setup_panic!(human_panic::Metadata::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )
    .homepage(env!("CARGO_PKG_HOMEPAGE"))
    .support("Open a support request at https://radicle.zulipchat.com/ or file an issue via Radicle itself, or e-mail to team@radicle.xyz"));

    if let Some(lvl) = radicle::logger::env_level() {
        let logger = Box::new(radicle::logger::Logger::new(lvl));
        log::set_boxed_logger(logger).expect("no other logger should have been set already");
        log::set_max_level(lvl.to_level_filter());
    }
    if let Err(e) = radicle::io::set_file_limit(4096) {
        log::warn!(target: "cli", "Unable to set open file limit: {e}");
    }
    let CliArgs { command } = CliArgs::parse();
    match run(command, term::DefaultContext) {
        Ok(_) => process::exit(0),
        Err(err) => {
            term::error(format!("{err}"));
            process::exit(1);
        }
    }
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

fn run(command: Command, ctx: impl term::Context) -> Result<(), anyhow::Error> {
    match command {
        Command::Auth(args) => term::run_command_fn(auth::run, args, ctx),
        Command::Block(args) => term::run_command_fn(block::run, args, ctx),
        Command::Checkout(args) => term::run_command_fn(checkout::run, args, ctx),
        Command::Clean(args) => term::run_command_fn(clean::run, args, ctx),
        Command::Clone(args) => term::run_command_fn(clone::run, args, ctx),
        Command::Cob(args) => term::run_command_fn(cob::run, args, ctx),
        Command::Config(args) => term::run_command_fn(config::run, args, ctx),
        Command::Debug(args) => term::run_command_fn(debug::run, args, ctx),
        Command::Follow(args) => term::run_command_fn(follow::run, args, ctx),
        Command::Fork(args) => term::run_command_fn(fork::run, args, ctx),
        Command::Id(args) => term::run_command_fn(id::run, args, ctx),
        Command::Inbox(args) => term::run_command_fn(inbox::run, args, ctx),
        Command::Init(args) => term::run_command_fn(init::run, args, ctx),
        Command::Inspect(args) => term::run_command_fn(inspect::run, args, ctx),
        Command::Issue(args) => term::run_command_fn(issue::run, args, ctx),
        Command::Ls(args) => term::run_command_fn(ls::run, args, ctx),
        Command::Node(args) => term::run_command_fn(node::run, args, ctx),
        Command::Patch(args) => term::run_command_fn(patch::run, args, ctx),
        Command::Path(args) => term::run_command_fn(path::run, args, ctx),
        Command::Publish(args) => term::run_command_fn(publish::run, args, ctx),
        Command::Remote(args) => term::run_command_fn(remote::run, args, ctx),
        Command::Seed(args) => term::run_command_fn(seed::run, args, ctx),
        Command::RadSelf(args) => term::run_command_fn(rad_self::run, args, ctx),
        Command::Stats(args) => term::run_command_fn(stats::run, args, ctx),
        Command::Sync(args) => term::run_command_fn(sync::run, args, ctx),
        Command::Unblock(args) => term::run_command_fn(unblock::run, args, ctx),
        Command::Unfollow(args) => term::run_command_fn(unfollow::run, args, ctx),
        Command::Unseed(args) => term::run_command_fn(unseed::run, args, ctx),
        Command::Watch(args) => term::run_command_fn(watch::run, args, ctx),
        Command::Version { json } => write_version(json),
        Command::External(mut args) => {
            let exe = args.remove(0);

            // This command is deprecated and delegates to `git diff`.
            // Even before it was deprecated, it was not printed by
            // `rad -h`.
            //
            // Since it is external, `--help` will delegate to `git diff --help`.
            if exe == "diff" {
                return diff::run(args);
            }

            let exe = format!("{NAME}-{exe:?}");
            let status = process::Command::new(&exe).args(&args).status();

            match status {
                Ok(status) => {
                    if !status.success() {
                        return Err(anyhow!("`{exe}` exited with an error."));
                    }
                    Ok(())
                }
                Err(err) => {
                    if let ErrorKind::NotFound = err.kind() {
                        Err(anyhow!(
                            "`{exe}` is not a command. See `rad --help` for a list of commands.",
                        ))
                    } else {
                        Err(err.into())
                    }
                }
            }
        }
    }
}

#![allow(clippy::or_fun_call)]
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use anyhow::anyhow;
use serde::Serialize;

use radicle::Profile;

use crate::terminal as term;
use crate::terminal::args::{Args, Help};

pub const NAME: &str = "rad";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const GIT_HEAD: &str = env!("GIT_HEAD");

pub const HELP: Help = Help {
    name: "debug",
    description: "Write out information to help debug your Radicle node remotely",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad debug

    Run this if you are reporting a problem in Radicle. The output is
    helpful for Radicle developers to debug your problem remotely. The
    output is meant to not include any sensitive information, but
    please check it, and then forward to the Radicle developers.

"#,
};

#[derive(Debug)]
pub struct Options {}

impl Args for Options {
    fn from_args(_args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        Ok((Options {}, vec![]))
    }
}

pub fn run(_options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    match ctx.profile() {
        Ok(profile) => debug(&profile),
        Err(e) => {
            eprintln!("ERROR: {e}");
            Err(e)
        }
    }
}

// Collect information about the local Radicle installation and write
// it out.
fn debug(profile: &Profile) -> anyhow::Result<()> {
    let env = HashMap::from_iter(env::vars().filter_map(|(k, v)| {
        if k == "RAD_PASSPHRASE" {
            Some((k, "<REDACTED>".into()))
        } else if k.starts_with("RAD_") || k.starts_with("SSH_") {
            Some((k, v))
        } else {
            None
        }
    }));

    let debug = DebugInfo {
        rad_version: VERSION,
        radicle_node_version: stdout_of("radicle-node", &["--version"])
            .unwrap_or("<unknown>".into()),
        git_remote_rad_version: stdout_of("git-remote-rad", &["--version"])
            .unwrap_or("<unknown>".into()),
        git_version: stdout_of("git", &["--version"]).unwrap_or("<unknown>".into()),
        ssh_version: stderr_of("ssh", &["-V"]).unwrap_or("<unknown>".into()),
        git_head: GIT_HEAD,
        log: LogFile::new(profile.node().join("node.log")),
        old_log: LogFile::new(profile.node().join("node.log.old")),
        operating_system: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        env,
    };

    println!("{}", serde_json::to_string_pretty(&debug).unwrap());

    Ok(())
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct DebugInfo {
    rad_version: &'static str,
    radicle_node_version: String,
    git_remote_rad_version: String,
    git_version: String,
    ssh_version: String,
    git_head: &'static str,
    log: LogFile,
    old_log: LogFile,
    operating_system: &'static str,
    arch: &'static str,
    env: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct LogFile {
    filename: PathBuf,
    exists: bool,
    len: Option<u64>,
}

impl LogFile {
    fn new(filename: PathBuf) -> Self {
        Self {
            filename: filename.clone(),
            exists: filename.exists(),
            len: if let Ok(meta) = filename.metadata() {
                Some(meta.len())
            } else {
                None
            },
        }
    }
}

fn output_of(bin: &str, args: &[&str]) -> anyhow::Result<(String, String)> {
    let output = Command::new(bin).args(args).output()?;
    if !output.status.success() {
        return Err(anyhow!("command failed: {bin:?} {args:?}"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok((stdout, stderr))
}

fn stdout_of(bin: &str, args: &[&str]) -> anyhow::Result<String> {
    let (stdout, _) = output_of(bin, args)?;
    Ok(stdout)
}

fn stderr_of(bin: &str, args: &[&str]) -> anyhow::Result<String> {
    let (_, stderr) = output_of(bin, args)?;
    Ok(stderr)
}

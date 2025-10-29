mod args;

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::process::Command;

use anyhow::anyhow;
use serde::Serialize;

use radicle::Profile;

use crate::terminal as term;

pub use args::Args;

pub const NAME: &str = "rad";
pub const VERSION: &str = env!("RADICLE_VERSION");
pub const DESCRIPTION: &str = "Radicle command line interface";
pub const GIT_HEAD: &str = env!("GIT_HEAD");

pub fn run(_args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    match ctx.profile() {
        Ok(profile) => debug(Some(&profile)),
        Err(e) => {
            eprintln!("ERROR: Could not load Radicle profile: {e}");
            debug(None)
        }
    }
}

// Collect information about the local Radicle installation and write
// it out.
fn debug(profile: Option<&Profile>) -> anyhow::Result<()> {
    let env = BTreeMap::from_iter(env::vars().filter_map(|(k, v)| {
        if k == "RAD_PASSPHRASE" {
            Some((k, "<REDACTED>".into()))
        } else if k.starts_with("RAD_") || k.starts_with("SSH_") || k == "PATH" || k == "SHELL" {
            Some((k, v))
        } else {
            None
        }
    }));

    let debug = DebugInfo {
        rad_exe: std::env::current_exe().ok(),
        rad_version: VERSION,
        radicle_node_version: stdout_of("radicle-node", &["--version"])
            .unwrap_or("radicle-node <unknown>".into()),
        git_remote_rad_version: stdout_of("git-remote-rad", &["--version"])
            .unwrap_or("git-remote-rad <unknown>".into()),
        git_version: stdout_of("git", &["--version"]).unwrap_or("<unknown>".into()),
        ssh_version: stderr_of("ssh", &["-V"]).unwrap_or("<unknown>".into()),
        git_head: GIT_HEAD,
        log: profile.map(|p| LogFile::new(p.node().join("node.log"))),
        old_log: profile.map(|p| LogFile::new(p.node().join("node.log.old"))),
        operating_system: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        env,
        warnings: collect_warnings(profile),
    };

    println!("{}", serde_json::to_string_pretty(&debug).unwrap());

    Ok(())
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct DebugInfo {
    rad_exe: Option<PathBuf>,
    rad_version: &'static str,
    radicle_node_version: String,
    git_remote_rad_version: String,
    git_version: String,
    ssh_version: String,
    git_head: &'static str,
    log: Option<LogFile>,
    old_log: Option<LogFile>,
    operating_system: &'static str,
    arch: &'static str,
    env: BTreeMap<String, String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
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

fn collect_warnings(profile: Option<&Profile>) -> Vec<String> {
    match profile {
        Some(profile) => crate::warning::nodes_renamed(&profile.config),
        None => vec!["No Radicle profile found.".to_string()],
    }
}

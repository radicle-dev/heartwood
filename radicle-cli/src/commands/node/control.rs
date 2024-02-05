use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::{fs, io, path::Path, process, thread, time};

use localtime::LocalTime;

use radicle::node;
use radicle::node::{Address, ConnectResult, Handle as _, NodeId};
use radicle::Node;
use radicle::{profile, Profile};

use crate::terminal as term;
use crate::terminal::Element as _;

/// How long to wait for the node to start before returning an error.
pub const NODE_START_TIMEOUT: time::Duration = time::Duration::from_secs(6);
/// Node log file name.
pub const NODE_LOG: &str = "node.log";
/// Node log old file name, after rotation.
pub const NODE_LOG_OLD: &str = "node.log.old";

pub fn start(
    node: Node,
    daemon: bool,
    verbose: bool,
    mut options: Vec<OsString>,
    cmd: &Path,
    profile: &Profile,
) -> anyhow::Result<()> {
    if node.is_running() {
        term::success!("Node is already running.");
        return Ok(());
    }
    let envs = if profile.keystore.is_encrypted()? {
        // Ask passphrase here, otherwise it'll be a fatal error when running the daemon
        // without `RAD_PASSPHRASE`.
        let validator = term::io::PassphraseValidator::new(profile.keystore.clone());
        let passphrase = if let Some(phrase) = profile::env::passphrase() {
            phrase
        } else if let Ok(phrase) = term::io::passphrase(validator) {
            phrase
        } else {
            anyhow::bail!("your radicle passphrase is required to start your node");
        };
        Some((profile::env::RAD_PASSPHRASE, passphrase))
    } else {
        None
    };

    // Since we checked that the node is not running, it's safe to use `--force`
    // here.
    if !options.contains(&OsString::from("--force")) {
        options.push(OsString::from("--force"));
    }
    if daemon {
        let log = log_rotate(profile)?;
        let child = process::Command::new(cmd)
            .args(options)
            .envs(envs)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log.try_clone()?))
            .stderr(process::Stdio::from(log))
            .spawn()?;
        let pid = term::format::parens(term::format::dim(child.id()));

        if verbose {
            logs(0, Some(time::Duration::from_secs(1)), profile)?;
        } else {
            let started = time::Instant::now();
            let mut spinner = term::spinner(format!("Node starting.. {pid}"));

            loop {
                if node.is_running() {
                    spinner.message(format!("Node started {pid}"));
                    spinner.finish();

                    term::print(term::format::dim(
                        "To stay in sync with the network, leave the node running in the background.",
                    ));
                    term::info!(
                        "{} {}{}",
                        term::format::dim("To learn more, run"),
                        term::format::command("rad node --help"),
                        term::format::dim("."),
                    );
                    break;
                } else if started.elapsed() >= NODE_START_TIMEOUT {
                    anyhow::bail!(
                        "node failed to start. Try running it with `rad node start --foreground`, \
                        or check the logs with `rad node logs`"
                    );
                }
                thread::sleep(time::Duration::from_millis(60));
            }
        }
    } else {
        let mut child = process::Command::new(cmd)
            .args(options)
            .envs(envs)
            .spawn()?;

        child.wait()?;
    }

    Ok(())
}

pub fn stop(node: Node) -> anyhow::Result<()> {
    let mut spinner = term::spinner("Stopping node...");
    if node.shutdown().is_err() {
        spinner.error("node is not running");
    } else {
        spinner.message("Node stopped");
        spinner.finish();
    }
    Ok(())
}

pub fn logs(lines: usize, follow: Option<time::Duration>, profile: &Profile) -> anyhow::Result<()> {
    let logs = profile.home.node().join("node.log");

    let mut file = BufReader::new(File::open(logs)?);
    file.seek(SeekFrom::End(0))?;

    let mut tail = Vec::new();
    let mut nlines = 0;

    for i in (1..=file.stream_position()?).rev() {
        let mut buf = [0; 1];
        file.seek(SeekFrom::Start(i - 1))?;
        file.read_exact(&mut buf)?;

        if buf[0] == b'\n' {
            nlines += 1;
        }
        if nlines > lines {
            break;
        }
        tail.push(buf[0]);
    }
    tail.reverse();

    print!("{}", term::format::dim(String::from_utf8_lossy(&tail)));

    if let Some(timeout) = follow {
        file.seek(SeekFrom::End(0))?;

        let start = time::Instant::now();

        while start.elapsed() < timeout {
            let mut line = String::new();
            let len = file.read_line(&mut line)?;

            if len == 0 {
                thread::sleep(time::Duration::from_millis(250));
            } else {
                print!("{}", term::format::dim(line));
            }
        }
    }
    Ok(())
}

pub fn connect(
    node: &mut Node,
    nid: NodeId,
    addr: Address,
    timeout: time::Duration,
) -> anyhow::Result<()> {
    let spinner = term::spinner(format!(
        "Connecting to {}@{addr}...",
        term::format::node(&nid)
    ));
    match node.connect(
        nid,
        addr,
        node::ConnectOptions {
            persistent: true,
            timeout,
        },
    ) {
        Ok(ConnectResult::Connected) => spinner.finish(),
        Ok(ConnectResult::Disconnected { reason }) => spinner.error(reason),
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

pub fn status(node: &Node, profile: &Profile) -> anyhow::Result<()> {
    if node.is_running() {
        term::success!("Node is {}.", term::format::positive("running"));
    } else {
        term::info!("Node is {}.", term::format::negative("stopped"));
        term::info!(
            "To start it, run {}.",
            term::format::command("rad node start")
        );
        return Ok(());
    }

    let sessions = sessions(node)?;
    if let Some(table) = sessions {
        term::blank();
        table.print();
    }

    if profile.home.node().join("node.log").exists() {
        term::blank();
        // If we're running the node via `systemd` for example, there won't be a log file
        // and this will fail.
        logs(10, None, profile)?;
    }
    Ok(())
}

pub fn sessions(node: &Node) -> Result<Option<term::Table<4, term::Label>>, node::Error> {
    let sessions = node.sessions()?;
    if sessions.is_empty() {
        return Ok(None);
    }
    let mut table = term::Table::new(term::table::TableOptions::bordered());
    let now = LocalTime::now();

    table.push([
        term::format::bold("Peer").into(),
        term::format::bold("Address").into(),
        term::format::bold("State").into(),
        term::format::bold("Since").into(),
    ]);
    table.divider();

    for sess in sessions {
        let nid = term::format::tertiary(sess.nid).into();
        let (addr, state, time) = match sess.state {
            node::State::Initial => (
                term::Label::blank(),
                term::Label::from(term::format::dim("initial")),
                term::Label::blank(),
            ),
            node::State::Attempted => (
                sess.addr.to_string().into(),
                term::Label::from(term::format::tertiary("attempted")),
                term::Label::blank(),
            ),
            node::State::Connected { since, .. } => (
                sess.addr.to_string().into(),
                term::Label::from(term::format::positive("connected")),
                term::format::dim(now - since).into(),
            ),
            node::State::Disconnected { retry_at, .. } => (
                sess.addr.to_string().into(),
                term::Label::from(term::format::negative("disconnected")),
                term::format::dim(retry_at - now).into(),
            ),
        };
        table.push([nid, addr, state, time]);
    }
    Ok(Some(table))
}

pub fn config(node: &Node) -> anyhow::Result<()> {
    let cfg = node.config()?;
    let cfg = serde_json::to_string_pretty(&cfg)?;

    println!("{cfg}");

    Ok(())
}

fn log_rotate(profile: &Profile) -> io::Result<File> {
    let base = profile.home.node();
    if base.join(NODE_LOG).exists() {
        // Let this fail, eg. if the file doesn't exist.
        fs::remove_file(base.join(NODE_LOG_OLD)).ok();
        fs::rename(base.join(NODE_LOG), base.join(NODE_LOG_OLD))?;
    }

    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(base.join(NODE_LOG))?;

    Ok(log)
}

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::{process, thread, time};

use anyhow::Context as _;
use localtime::LocalTime;

use radicle::node;
use radicle::node::{Address, Handle as _, NodeId};
use radicle::Node;
use radicle::{profile, Profile};

use crate::terminal as term;
use crate::terminal::Element as _;

pub fn start(
    node: Node,
    daemon: bool,
    mut options: Vec<OsString>,
    profile: &Profile,
) -> anyhow::Result<()> {
    if node.is_running() {
        term::success!("Node is already running.");
        return Ok(());
    }
    let envs = if profile.keystore.is_encrypted()? {
        // Ask passphrase here, otherwise it'll be a fatal error when running the daemon
        // without `RAD_PASSPHRASE`. To keep things consistent, we also use this in foreground mode.
        let passphrase = term::io::passphrase(profile::env::RAD_PASSPHRASE)
            .context(format!("`{}` must be set", profile::env::RAD_PASSPHRASE))?;

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
        let log = OpenOptions::new()
            .append(true)
            .create(true)
            .open(profile.home.node().join("node.log"))?;

        process::Command::new("radicle-node")
            .args(options)
            .envs(envs)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log))
            .stderr(process::Stdio::null())
            .spawn()?;

        logs(0, Some(time::Duration::from_secs(1)), profile)?;
    } else {
        let mut child = process::Command::new("radicle-node")
            .args(options)
            .envs(envs)
            .spawn()?;

        child.wait()?;
    }

    Ok(())
}

pub fn stop(node: Node) -> anyhow::Result<()> {
    let spinner = term::spinner("Stopping node...");
    if node.shutdown().is_err() {
        spinner.error("node is not running");
    } else {
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

pub fn connect(node: &mut Node, nid: NodeId, addr: Address) -> anyhow::Result<()> {
    let spinner = term::spinner(format!(
        "Connecting to {}@{addr}...",
        term::format::node(&nid)
    ));
    if let Err(err) = node.connect(nid, addr.clone(), node::ConnectOptions { persistent: true }) {
        spinner.error(format!(
            "Failed to connect to {}@{}: {}",
            term::format::node(&nid),
            term::format::secondary(addr),
            err,
        ))
    } else {
        spinner.finish()
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

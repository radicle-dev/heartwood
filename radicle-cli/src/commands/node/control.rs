use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::{process, thread, time};

use anyhow::Context as _;

use radicle::node::{Address, Handle as _, NodeId};
use radicle::Node;
use radicle::{profile, Profile};

use crate::terminal as term;

pub fn start(daemon: bool, options: Vec<OsString>, profile: &Profile) -> anyhow::Result<()> {
    // Ask passphrase here, otherwise it'll be a fatal error when running the daemon
    // without `RAD_PASSPHRASE`. To keep things consistent, we also use this in foreground mode.
    let passphrase = term::io::passphrase(profile::env::RAD_PASSPHRASE)
        .context(format!("`{}` must be set", profile::env::RAD_PASSPHRASE))?;

    if daemon {
        let log = OpenOptions::new()
            .append(true)
            .create(true)
            .open(profile.home.node().join("node.log"))?;

        process::Command::new("radicle-node")
            .args(options)
            .env(profile::env::RAD_PASSPHRASE, passphrase)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log))
            .stderr(process::Stdio::null())
            .spawn()?;

        logs(0, Some(time::Duration::from_secs(1)), profile)?;
    } else {
        let mut child = process::Command::new("radicle-node")
            .args(options)
            .env(profile::env::RAD_PASSPHRASE, passphrase)
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
    file.seek(SeekFrom::End(-1))?;

    let mut tail = Vec::new();
    let mut nlines = 0;

    for i in (0..=file.stream_position()?).rev() {
        let mut buf = [0; 1];
        file.seek(SeekFrom::Start(i))?;
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
    if let Err(err) = node.connect(nid, addr.clone()) {
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
        term::success!("The node is {}", term::format::positive("running"));
    } else {
        term::info!("The node is {}", term::format::negative("stopped"));
    }
    if profile.home.node().join("node.log").exists() {
        term::blank();
        // If we're running the node via `systemd` for example, there won't be a log file
        // and this will fail.
        logs(10, None, profile)?;
    }
    Ok(())
}

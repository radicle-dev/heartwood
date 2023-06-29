use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::time::Duration;
use std::{process, thread};

use radicle::node::{Address, Handle as _, NodeId};
use radicle::Node;

use crate::terminal as term;

pub fn start(daemon: bool, options: Vec<OsString>) -> anyhow::Result<()> {
    if daemon {
        let home = radicle::profile::home()?;
        let log = OpenOptions::new()
            .append(true)
            .create(true)
            .open(home.node().join("node.log"))?;
        process::Command::new("radicle-node")
            .args(options)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log))
            .stderr(process::Stdio::null())
            .spawn()?;
    } else {
        let mut child = process::Command::new("radicle-node")
            .args(options)
            .spawn()?;

        child.wait()?;
    }

    Ok(())
}

pub fn stop(node: Node) -> anyhow::Result<()> {
    let spinner = term::spinner("Stopping the node...");
    if let Err(err) = node.shutdown() {
        spinner.error(format!("Error occurred while shutting down node: {err}"));
    } else {
        spinner.finish();
    }
    Ok(())
}

pub fn logs(lines: usize) -> anyhow::Result<()> {
    let home = radicle::profile::home()?;
    let logs = home.node().join("node.log");

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

    print!("{}", String::from_utf8_lossy(&tail));

    file.seek(SeekFrom::End(0))?;
    loop {
        let mut line = String::new();
        let len = file.read_line(&mut line)?;
        if len == 0 {
            thread::sleep(Duration::from_millis(250));
        } else {
            print!("{line}");
        }
    }
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

pub fn status(node: &Node) {
    if node.is_running() {
        term::success!("The node is {}", term::format::positive("running"));
    } else {
        term::info!("The node is {}", term::format::negative("stopped"));
    }
}

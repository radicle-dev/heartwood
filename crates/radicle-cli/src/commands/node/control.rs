use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::{path::Path, process, thread, time};

use anyhow::{Context, anyhow};
use localtime::LocalTime;

use radicle::Node;
use radicle::node;
use radicle::node::{Address, ConnectResult, Handle as _, NodeId};
use radicle::profile::env::RAD_PASSPHRASE;
use radicle::{Profile, profile};

use crate::commands::node::logs::{LogRotatorFileSystem, Rotated};
use crate::terminal as term;
use crate::terminal::Element as _;

/// How long to wait for the node to start before returning an error.
pub const NODE_START_TIMEOUT: time::Duration = time::Duration::from_secs(6);

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
        } else if let Some(phrase) = term::io::passphrase(validator)? {
            phrase
        } else {
            anyhow::bail!(
                "A passphrase is required to read your Radicle key in order to start the node. Unable to continue. Consider setting the environment variable `{RAD_PASSPHRASE}`."
            );
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

    let Rotated {
        path: log_path,
        log: log_file,
    } = LogRotatorFileSystem::from_profile(profile).rotate()?;

    if daemon {
        let child = process::Command::new(cmd)
            .args(options)
            .envs(envs)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::from(log_file.try_clone()?))
            .stderr(process::Stdio::from(log_file))
            .spawn()
            .map_err(|e| anyhow!("failed to start node process {cmd:?}: {e}"))?;
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
        // Write a hint to the log file, but swallow any errors.
        let mut log_file = log_file;
        let _ = log_file.write_all(format!("radicle-node started in foreground, no further log messages are written to '{}' (this file).\n", log_path.display()).as_bytes());

        let mut child = process::Command::new(cmd)
            .args(options)
            .envs(envs)
            .spawn()
            .map_err(|e| anyhow!("failed to start node process {cmd:?}: {e}"))?;

        child.wait()?;
    }

    Ok(())
}

pub fn stop(node: Node, profile: &Profile) {
    let mut spinner = term::spinner("Stopping node...");
    if node.shutdown().is_err() {
        spinner.error("node is not running");
    } else {
        spinner.message("Node stopped");
        spinner.finish();
    }
    let rotator = LogRotatorFileSystem::from_profile(profile);
    rotator.remove().ok();
}

pub fn debug(node: &mut Node) -> anyhow::Result<()> {
    let json = node.debug()?;
    term::json::to_pretty(&json, Path::new("debug.json"))?.print();

    Ok(())
}

pub fn logs(lines: usize, follow: Option<time::Duration>, profile: &Profile) -> anyhow::Result<()> {
    let logs_path = profile.home.node().join("node.log");
    let mut file = File::open(logs_path.clone())
        .map(BufReader::new)
        .with_context(|| {
            format!(
                "Failed to read log file at '{}'. Did you start the node with `rad node start`? \
                If the node was started through a process manager, check its logs instead.",
                logs_path.display()
            )
        })?;

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

    term::print_inline(term::format::dim(String::from_utf8_lossy(&tail)));

    if let Some(timeout) = follow {
        file.seek(SeekFrom::End(0))?;

        let start = time::Instant::now();

        while start.elapsed() < timeout {
            let mut line = String::new();
            let len = file.read_line(&mut line)?;

            if len == 0 {
                thread::sleep(time::Duration::from_millis(250));
            } else {
                term::print_inline(term::format::dim(line));
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
        term::format::node_id_human_compact(&nid)
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

pub fn connect_many(
    node: &mut Node,
    nid: NodeId,
    addrs: Vec<Address>,
    timeout: time::Duration,
) -> anyhow::Result<()> {
    let mut spinner = term::spinner("Connecting...");
    let mut errors = HashMap::new();
    for addr in addrs {
        spinner.message(format!(
            "Connecting to {}@{addr}...",
            term::format::node_id_human_compact(&nid)
        ));
        match node.connect(
            nid,
            addr.clone(),
            node::ConnectOptions {
                persistent: true,
                timeout,
            },
        ) {
            Ok(ConnectResult::Connected) => {
                spinner.finish();
                return Ok(());
            }
            Ok(ConnectResult::Disconnected { reason }) => {
                errors.insert(addr, reason);
            }
            Err(err) => {
                errors.insert(addr, err.to_string());
            }
        }
    }
    spinner.failed();
    for (addr, err) in errors {
        term::error(format!("Failed to connect to {addr}: {err}"));
    }
    Ok(())
}

pub fn status(node: &Node, profile: &Profile) -> anyhow::Result<()> {
    for warning in crate::warning::config_warnings(&profile.config) {
        term::warning(warning);
    }

    if !node.is_running() {
        term::info!("Node is {}.", term::format::negative("stopped"));
        term::info!(
            "To start it, run {}.",
            term::format::command("rad node start")
        );
        return Ok(());
    }

    let listen = node
        .listen_addrs()?
        .into_iter()
        .map(|addr| addr.to_string())
        .collect::<Vec<_>>();

    let nid = node.nid()?;
    let nid = if &nid == profile.id() {
        term::format::tertiary(term::format::node_id_human(&nid))
    } else {
        term::format::yellow(term::format::node_id_human(&nid)).bold()
    };

    if listen.is_empty() {
        term::success!(
            "Node is {} with Node ID {} and {} configured to listen for inbound connections.",
            term::format::positive("running"),
            nid,
            term::Paint::new("not").italic()
        );
    } else {
        term::success!(
            "Node is {} with Node ID {} and listening for inbound connections on {}.",
            term::format::positive("running"),
            nid,
            listen.join(", ")
        );
    }

    let sessions = sessions(node)?;
    if let Some(table) = sessions {
        term::blank();
        table.print();
    }

    if profile.hints() {
        const COLUMN_WIDTH: usize = 12;
        let status = format!(
            "\n{:>4} … {}\n       {}   {}\n       {}   {}",
            state_label().fg(radicle_term::Color::White),
            term::format::dim("Status:"),
            format_args!(
                "{} {:width$}",
                state_connected(),
                term::format::dim("… connected"),
                width = COLUMN_WIDTH,
            ),
            format_args!(
                "{} {}",
                state_disconnected(),
                term::format::dim("… disconnected")
            ),
            format_args!(
                "{} {:width$}",
                state_attempted(),
                term::format::dim("… attempted"),
                width = COLUMN_WIDTH,
            ),
            format_args!("{} {}", state_initial(), term::format::dim("… initial")),
        );
        let link_direction = format!(
            "\n{:>4} … {}\n       {}   {}",
            link_direction_label(),
            term::format::dim("Link Direction:"),
            format_args!(
                "{} {:width$}",
                link_direction_inbound(),
                term::format::dim("… inbound"),
                width = COLUMN_WIDTH,
            ),
            format_args!(
                "{} {}",
                link_direction_outbound(),
                term::format::dim("… outbound")
            ),
        );
        term::hint(status + &link_direction);
    }

    if profile.home.node().join("node.log").exists() {
        term::blank();
        // If we're running the node via `systemd` for example, there won't be a log file
        // and this will fail.
        logs(10, None, profile)?;
    }
    Ok(())
}

pub fn sessions(node: &Node) -> Result<Option<term::Table<5, term::Label>>, node::Error> {
    let sessions = node.sessions()?;
    if sessions.is_empty() {
        return Ok(None);
    }
    let mut table = term::Table::new(term::table::TableOptions::bordered());
    let now = LocalTime::now();

    table.header([
        term::format::bold("Node ID").into(),
        term::format::bold("Address").into(),
        state_label().into(),
        link_direction_label().bold().into(),
        term::format::bold("Since").into(),
    ]);
    table.divider();

    table.extend(sessions.into_iter().map(|sess| {
        let nid = term::format::tertiary(term::format::node_id_human(&sess.nid)).into();
        let (addr, state, time) = match sess.state {
            node::State::Initial => (
                term::Label::blank(),
                term::Label::from(state_initial()),
                term::Label::blank(),
            ),
            node::State::Attempted => (
                sess.addr.display_compact().to_string().into(),
                term::Label::from(state_attempted()),
                term::Label::blank(),
            ),
            node::State::Connected { since, .. } => (
                sess.addr.display_compact().to_string().into(),
                term::Label::from(state_connected()),
                term::format::dim(now - since).into(),
            ),
            node::State::Disconnected { since, .. } => (
                sess.addr.display_compact().to_string().into(),
                term::Label::from(state_disconnected()),
                term::format::dim(now - since).into(),
            ),
        };
        let direction = match sess.link {
            node::Link::Inbound => term::Label::from(link_direction_inbound()),
            node::Link::Outbound => term::Label::from(link_direction_outbound()),
        };

        [nid, addr, state, direction, time]
    }));

    Ok(Some(table))
}

pub fn config(node: &Node) -> anyhow::Result<()> {
    let cfg = node.config()?;
    let cfg = serde_json::to_string_pretty(&cfg)?;

    term::print(cfg);

    Ok(())
}

fn state_label() -> term::Paint<String> {
    term::Paint::from("?".to_string())
}

fn state_initial() -> term::Paint<String> {
    term::format::dim("•".to_string())
}

fn state_attempted() -> term::Paint<String> {
    term::PREFIX_WARNING.into()
}

fn state_connected() -> term::Paint<String> {
    term::PREFIX_SUCCESS.into()
}

fn state_disconnected() -> term::Paint<String> {
    term::PREFIX_ERROR.into()
}

fn link_direction_label() -> term::Paint<String> {
    term::Paint::from("⤭".to_string())
}

fn link_direction_inbound() -> term::Paint<String> {
    term::format::yellow("↘".to_string())
}

fn link_direction_outbound() -> term::Paint<String> {
    term::format::dim("↗".to_string())
}

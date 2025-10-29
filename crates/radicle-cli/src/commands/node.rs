mod args;
mod commands;
pub mod control;
mod events;
mod logs;
pub mod routing;

use std::{process, time};

use radicle::node::address::Store as AddressStore;
use radicle::node::config::ConnectAddress;
use radicle::node::routing::Store;
use radicle::node::Handle as _;
use radicle::node::Node;

use crate::commands::node::args::Only;
use crate::terminal as term;
use crate::terminal::Element as _;
use crate::warning;

pub use args::Args;
use args::{Addr, Command};

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = Node::new(profile.socket());

    let command = args.command.unwrap_or_default();

    match command {
        Command::Connect { addr, timeout } => {
            let timeout = timeout
                .map(time::Duration::from_secs)
                .unwrap_or(time::Duration::MAX);
            match addr {
                Addr::Peer(addr) => control::connect(&mut node, addr.id, addr.addr, timeout)?,
                Addr::Node(nid) => {
                    let db = profile.database()?;
                    let addresses = db
                        .addresses_of(&nid)?
                        .into_iter()
                        .map(|ka| ka.addr)
                        .collect();
                    control::connect_many(&mut node, nid, addresses, timeout)?;
                }
            }
        }
        Command::Config { addresses } => {
            if addresses {
                let cfg = node.config()?;
                for addr in cfg.external_addresses {
                    term::print(ConnectAddress::from((*profile.id(), addr)).to_string());
                }
            } else {
                control::config(&node)?;
            }
        }
        Command::Db(op) => {
            commands::db(&profile, op)?;
        }
        Command::Debug => {
            control::debug(&mut node)?;
        }
        Command::Sessions => {
            warning::deprecated("rad node sessions", "rad node status");
            let sessions = control::sessions(&node)?;
            if let Some(table) = sessions {
                table.print();
            }
        }
        Command::Events { timeout, count } => {
            let count = count.unwrap_or(usize::MAX);
            let timeout = timeout
                .map(time::Duration::from_secs)
                .unwrap_or(time::Duration::MAX);

            events::run(node, count, timeout)?;
        }
        Command::Routing { rid, nid, json } => {
            let store = profile.database()?;
            routing::run(&store, rid, nid, json)?;
        }
        Command::Logs { lines } => control::logs(lines, Some(time::Duration::MAX), &profile)?,
        Command::Start {
            foreground,
            options,
            path,
            verbose,
        } => {
            control::start(node, !foreground, verbose, options, &path, &profile)?;
        }
        Command::Inventory { nid } => {
            let nid = nid.as_ref().unwrap_or(profile.id());
            for rid in profile.routing()?.get_inventory(nid)? {
                println!("{}", term::format::tertiary(rid));
            }
        }
        Command::Status {
            only: Some(Only::Nid),
        } => {
            if node.is_running() {
                term::print(term::format::node_id_human(&node.nid()?));
            } else {
                process::exit(2);
            }
        }
        Command::Status { only: None } => {
            control::status(&node, &profile)?;
        }
        Command::Stop => {
            control::stop(node, &profile);
        }
    }

    Ok(())
}

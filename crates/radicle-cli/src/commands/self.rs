#[path = "self/args.rs"]
mod args;

pub use args::Args;

use radicle::crypto::ssh;
use radicle::node::Handle as _;
use radicle::{Node, Profile};

use crate::terminal as term;
use crate::terminal::Element as _;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;

    if args.did {
        term::print(profile.did());
    } else if args.alias {
        term::print(profile.config.alias());
    } else if args.home {
        term::print(profile.home().path().display());
    } else if args.ssh_key {
        term::print(ssh::fmt::key(profile.id()));
    } else if args.config {
        term::print(profile.home.config().display());
    } else if args.ssh_fingerprint {
        term::print(ssh::fmt::fingerprint(profile.id()));
    } else if args.nid {
        crate::warning::deprecated("rad self --nid", "rad node status --only nid");
        term::print(
            Node::new(profile.socket())
                .nid()
                .ok()
                .unwrap_or_else(|| *profile.id()),
        );
    } else {
        all(&profile)?
    }

    Ok(())
}

fn all(profile: &Profile) -> anyhow::Result<()> {
    let mut table = term::Table::<2, term::Label>::default();

    table.push([
        term::format::style("Alias").into(),
        term::format::primary(profile.config.alias()).into(),
    ]);

    let did = profile.did();
    table.push([
        term::format::style("DID").into(),
        term::format::tertiary(did).into(),
    ]);

    let socket = profile.socket();
    let node = if Node::new(&socket).is_running() {
        term::format::positive(format!("running ({})", socket.display()))
    } else {
        term::format::negative("not running".to_string())
    };
    table.push([term::format::style("Node").into(), node.to_string().into()]);

    let ssh_agent = match ssh::agent::Agent::connect() {
        Ok(c) => term::format::positive(format!(
            "running ({})",
            c.path()
                .map(|p| p.display().to_string())
                .unwrap_or(String::from("?"))
        )),
        Err(e) if e.is_not_running() => term::format::yellow(String::from("not running")),
        Err(e) => term::format::negative(format!("error: {e}")),
    };
    table.push([
        term::format::style("SSH").into(),
        ssh_agent.to_string().into(),
    ]);

    let id = profile.id();
    let ssh_short = ssh::fmt::fingerprint(id);
    table.push([
        term::format::style("├╴Key (hash)").into(),
        term::format::tertiary(ssh_short).into(),
    ]);

    let ssh_long = ssh::fmt::key(id);
    table.push([
        term::format::style("└╴Key (full)").into(),
        term::format::tertiary(ssh_long).into(),
    ]);

    let home = profile.home();
    table.push([
        term::format::style("Home").into(),
        term::format::tertiary(home.path().display()).into(),
    ]);

    let config_path = profile.home.config();
    table.push([
        term::format::style("├╴Config").into(),
        term::format::tertiary(config_path.display()).into(),
    ]);

    let storage_path = profile.home.storage();
    table.push([
        term::format::style("├╴Storage").into(),
        term::format::tertiary(storage_path.display()).into(),
    ]);

    let keys_path = profile.home.keys();
    table.push([
        term::format::style("├╴Keys").into(),
        term::format::tertiary(keys_path.display()).into(),
    ]);

    table.push([
        term::format::style("└╴Node").into(),
        term::format::tertiary(profile.home.node().display()).into(),
    ]);

    table.print();

    Ok(())
}

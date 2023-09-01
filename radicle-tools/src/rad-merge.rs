use std::collections::HashSet;
use std::env;

use anyhow::anyhow;
use radicle::cob::patch::{PatchId, Patches};
use radicle::git::Oid;
use radicle::storage::ReadStorage;
use radicle_cli::terminal as term;

fn main() -> anyhow::Result<()> {
    let pid: PatchId = env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("usage: rad-merge <patch-id>"))?
        .parse()?;
    let profile = radicle::Profile::load()?;
    let (working, rid) = radicle::rad::cwd()?;
    let stored = profile.storage.repository(rid)?;
    let mut patches = Patches::open(&stored)?;
    let mut patch = patches.get_mut(&pid)?;

    if patch.is_merged() {
        anyhow::bail!("fatal: patch {pid} is already merged");
    }
    let (revision, r) = patch.latest();
    let head = r.head();

    let mut revwalk = stored.backend.revwalk()?;
    revwalk.push_head()?;

    let commits = revwalk
        .map(|r| r.map(Oid::from))
        .collect::<Result<HashSet<Oid>, _>>()?;

    if !commits.contains(&head) {
        anyhow::bail!("fatal: patch head {head} is not in default branch");
    }
    let signer = term::signer(&profile)?;

    patch
        .merge(*revision, head, &signer)?
        .cleanup(&working, &signer)?;

    println!("✓ Patch {pid} merged at commit {head}");
    println!("You may now run `rad sync --announce`.");

    Ok(())
}

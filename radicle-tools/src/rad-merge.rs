use std::collections::HashSet;
use std::env;

use anyhow::{anyhow, bail};
use radicle::cob::migrate;
use radicle::cob::patch::{PatchId, RevisionId};
use radicle::git::Oid;
use radicle::storage::ReadStorage;
use radicle_cli::terminal as term;

fn main() -> anyhow::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let (pid, rev) = match args.as_slice() {
        [pid, rev] => {
            let pid: PatchId = pid.parse()?;
            let rev: Oid = rev.parse()?;

            (pid, Some(RevisionId::from(rev)))
        }
        [pid] => {
            let pid: PatchId = pid.parse()?;

            (pid, None)
        }
        _ => bail!("usage: rad-merge <patch-id> [<revision-id>]"),
    };
    let profile = radicle::Profile::load()?;
    let (working, rid) = radicle::rad::cwd()?;
    let stored = profile.storage.repository(rid)?;
    let mut patches = profile.patches_mut(&stored, migrate::ignore)?;
    let mut patch = patches.get_mut(&pid)?;

    if patch.is_merged() {
        anyhow::bail!("fatal: patch {pid} is already merged");
    }
    let (revision, r) = if let Some(id) = rev {
        let r = patch
            .revision(&id)
            .ok_or_else(|| anyhow!("revision {id} not found"))?;
        (id, r)
    } else {
        patch.latest()
    };
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
        .merge(revision, head, &signer)?
        .cleanup(&working, &signer)?;

    println!("✓ Patch {pid} merged at commit {head}");
    println!("You may now run `rad sync --announce`.");

    Ok(())
}

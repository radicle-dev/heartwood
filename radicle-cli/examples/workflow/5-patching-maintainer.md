Back to being the project maintainer.

Changes have been proposed by another peer via a radicle patch. To track
changes from another peer, we must first follow them, and then create
a tracking branch in our working copy. The `rad remote add` command does all
of this.

```
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --sync --fetch
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkt67…v4N1tRk@[..]..
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
```

The contributor's changes are now visible to us.

```
$ rad inbox --sort-by id
╭────────────────────────────────────────────────────────────────────────────╮
│ heartwood                                                                  │
├────────────────────────────────────────────────────────────────────────────┤
│ 001   ●   3b2f7e6   flux capacitor underpowered   issue   open   bob   now │
│ 002   ●   3aa3bbf   Define power requirements     patch   open   bob   now │
╰────────────────────────────────────────────────────────────────────────────╯
$ git branch -r
  bob/patches/3aa3bbfbc4162e34ab6787b3508e7ec84166d182
  rad/master
$ rad patch show 3aa3bbf
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                                  │
│ Patch    3aa3bbfbc4162e34ab6787b3508e7ec84166d182                   │
│ Author   bob z6Mkt67…v4N1tRk                                        │
│ Head     27857ec9eb04c69cacab516e8bf4b5fd36090f66                   │
│ Commits  ahead 2, behind 0                                          │
│ Status   open                                                       │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by bob z6Mkt67…v4N1tRk (3e674d1) now                       │
│ ↑ updated to 8ea87be8cb7d590f381338348532200b230368af (27857ec) now │
╰─────────────────────────────────────────────────────────────────────╯
```

Wait! There's a mistake.  The REQUIREMENTS should be a markdown file.  Let's
quickly update the patch before incorporating the changes.  Updating it this
way will tell others about the corrections we needed before merging the
changes.

```
$ rad patch checkout 3aa3bbfbc4162e34ab6787b3508e7ec84166d182
✓ Switched to branch patch/3aa3bbf at revision 8ea87be
✓ Branch patch/3aa3bbf setup to track rad/patches/3aa3bbfbc4162e34ab6787b3508e7ec84166d182
$ git mv REQUIREMENTS REQUIREMENTS.md
$ git commit -m "Use markdown for requirements"
[patch/3aa3bbf f567f69] Use markdown for requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 rename REQUIREMENTS => REQUIREMENTS.md (100%)
```
``` (stderr)
$ git push rad -o no-sync -o patch.message="Use markdown for requirements"
✓ Patch 3aa3bbf updated to revision 83812f465f23c6f1262e4d526f52e5a5f02330a0
To compare against your previous revision 8ea87be, run:

   git range-diff f2de534[..] 27857ec[..] f567f69[..]

To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/3aa3bbf -> patches/3aa3bbfbc4162e34ab6787b3508e7ec84166d182
```

Great, all fixed up, lets accept and merge the code.

```
$ rad patch review 3aa3bbf --revision 83812f4 --accept
✓ Patch 3aa3bbf accepted
✓ Synced with 1 node(s)
$ git checkout master
Your branch is up to date with 'rad/master'.
$ git merge patch/3aa3bbf
Updating f2de534..f567f69
Fast-forward
 README.md       | 0
 REQUIREMENTS.md | 0
 2 files changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
 create mode 100644 REQUIREMENTS.md
```
``` (stderr)
$ git push rad master
✓ Patch 3aa3bbfbc4162e34ab6787b3508e7ec84166d182 merged at revision 83812f4
✓ Canonical head for refs/heads/master updated to f567f695d25b4e8fb63b5f5ad2a584529826e908
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..f567f69  master -> master
```

The patch is now merged and closed :).

```
$ rad patch show 3aa3bbf
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                                  │
│ Patch    3aa3bbfbc4162e34ab6787b3508e7ec84166d182                   │
│ Author   bob z6Mkt67…v4N1tRk                                        │
│ Head     27857ec9eb04c69cacab516e8bf4b5fd36090f66                   │
│ Commits  ahead 0, behind 1                                          │
│ Status   merged                                                     │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by bob z6Mkt67…v4N1tRk (3e674d1) now                       │
│ ↑ updated to 8ea87be8cb7d590f381338348532200b230368af (27857ec) now │
│ * revised by alice (you) in 83812f4 (f567f69) now                   │
│   └─ ✓ accepted by alice (you) now                                  │
│   └─ ✓ merged by alice (you) at revision 83812f4 (f567f69) now      │
╰─────────────────────────────────────────────────────────────────────╯
```

To publish our new state to the network, we simply push:

```
$ git push
```

Finally, we will close the issue that was opened for this
patch, marking it as solved:

```
$ rad issue state 3b2f7e6 --solved
✓ Issue 3b2f7e6 is now solved
✓ Synced with 1 node(s)
```

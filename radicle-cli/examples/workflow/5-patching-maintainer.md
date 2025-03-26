Back to being the project maintainer.

Changes have been proposed by another peer via a radicle patch. To track
changes from another peer, we must first follow them, and then create
a tracking branch in our working copy. The `rad remote add` command does all
of this.

```
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --sync --fetch
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk@[..]..
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
```

The contributor's changes are now visible to us.

```
$ rad inbox --sort-by id
╭────────────────────────────────────────────────────────────────────────────╮
│ heartwood                                                                  │
├────────────────────────────────────────────────────────────────────────────┤
│ 001   ●   9037b7a   flux capacitor underpowered   issue   open   bob   now │
│ 002   ●   e4934b6   Define power requirements     patch   open   bob   now │
╰────────────────────────────────────────────────────────────────────────────╯
$ git branch -r
  bob/patches/e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46
  rad/master
$ rad patch show e4934b6
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                                  │
│ Patch    e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46                   │
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
│ ↑ updated to 773b9aab58b11e9fa83d0ed0baca2bea6ff889c9 (27857ec) now │
╰─────────────────────────────────────────────────────────────────────╯
```

Wait! There's a mistake.  The REQUIREMENTS should be a markdown file.  Let's
quickly update the patch before incorporating the changes.  Updating it this
way will tell others about the corrections we needed before merging the
changes.

```
$ rad patch checkout e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46
✓ Switched to branch patch/e4934b6 at revision 773b9aa
✓ Branch patch/e4934b6 setup to track rad/patches/e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46
$ git mv REQUIREMENTS REQUIREMENTS.md
$ git commit -m "Use markdown for requirements"
[patch/e4934b6 f567f69] Use markdown for requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 rename REQUIREMENTS => REQUIREMENTS.md (100%)
```
``` (stderr)
$ git push rad -o no-sync -o patch.message="Use markdown for requirements"
✓ Patch e4934b6 updated to revision 9e458d00b2e9a26993113c48259781725e2cbee3
To compare against your previous revision 773b9aa, run:

   git range-diff f2de534[..] 27857ec[..] f567f69[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/e4934b6 -> patches/e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46
```

Great, all fixed up, lets accept and merge the code.

```
$ rad patch review e4934b6 --revision 9e458d00b2e9a26993113c48259781725e2cbee3 --accept
✓ Patch e4934b6 accepted
✓ Synced with 1 node(s)
$ git checkout master
Your branch is up to date with 'rad/master'.
$ git merge patch/e4934b6
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
✓ Patch e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46 merged at revision 9e458d0
✓ Canonical head updated to f567f695d25b4e8fb63b5f5ad2a584529826e908
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..f567f69  master -> master
```

The patch is now merged and closed :).

```
$ rad patch show e4934b6
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                                  │
│ Patch    e4934b6d9dbe01ce3c7fbb5b77a80d5f1dacdc46                   │
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
│ ↑ updated to 773b9aab58b11e9fa83d0ed0baca2bea6ff889c9 (27857ec) now │
│ * revised by alice (you) in 9e458d0 (f567f69) now                   │
│   └─ ✓ accepted by alice (you) now                                  │
│   └─ ✓ merged by alice (you) at revision 9e458d0 (f567f69) now      │
╰─────────────────────────────────────────────────────────────────────╯
```

To publish our new state to the network, we simply push:

```
$ git push
```

Finally, we will close the issue that was opened for this
patch, marking it as solved:

```
$ rad issue state 9037b7a --solved
✓ Issue 9037b7a is now solved
✓ Synced with 1 node(s)
```

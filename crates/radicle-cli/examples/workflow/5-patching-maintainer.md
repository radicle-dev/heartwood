Back to being the project maintainer.

Changes have been proposed by another peer via a Radicle patch. To track
changes from another peer, we must first follow them, and then create
a tracking branch in our working copy. The `rad remote add` command does all
of this.

```
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --sync --fetch
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

The contributor's changes are now visible to us.

```
$ rad inbox --sort-by id
╭────────────────────────────────────────────────────────────────────────────╮
│ heartwood                                                                  │
├────────────────────────────────────────────────────────────────────────────┤
│ 001   ●   9037b7a   flux capacitor underpowered   issue   open   bob   now │
│ 002   ●   da0b844   Define power requirements     patch   open   bob   now │
╰────────────────────────────────────────────────────────────────────────────╯
$ git branch -r
  bob/patches/da0b8447cf370a528b6c4a51ff9255eadd726edf
  rad/master
$ rad patch show da0b844
╭──────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                               │
│ Patch    da0b8447cf370a528b6c4a51ff9255eadd726edf                │
│ Author   bob z6Mkt67…v4N1tRk                                     │
│ Head     8083d4cbb2b297ade6a12962f8ddc118c3900dcf                │
│ Base     [..                                          ]          │
│ Target   master                                                  │
│ Commits  ahead 2, behind 0                                       │
│ Status   open                                                    │
│                                                                  │
│ See details.                                                     │
├──────────────────────────────────────────────────────────────────┤
│ 8083d4c Add README, just for the fun                             │
│ 602ba44 Define power requirements                                │
├──────────────────────────────────────────────────────────────────┤
│ ● Revision da0b844 @ [..   ]..602ba44 by bob z6Mkt67…v4N1tRk now │
│ ↑ Revision c6894b5 @ [..   ]..8083d4c by bob z6Mkt67…v4N1tRk now │
╰──────────────────────────────────────────────────────────────────╯
```

Wait! There's a mistake.  The REQUIREMENTS should be a markdown file.  Let's
quickly update the patch before incorporating the changes.  Updating it this
way will tell others about the corrections we needed before merging the
changes.

```
$ rad patch checkout da0b8447cf370a528b6c4a51ff9255eadd726edf
✓ Switched to branch patch/da0b844 at revision c6894b5
✓ Branch patch/da0b844 setup to track rad/patches/da0b8447cf370a528b6c4a51ff9255eadd726edf
$ git mv REQUIREMENTS REQUIREMENTS.md
$ git commit -m "Use markdown for requirements"
[patch/da0b844 6947d0d] Use markdown for requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 rename REQUIREMENTS => REQUIREMENTS.md (100%)
```
``` (stderr)
$ git push rad -o no-sync -o patch.message="Use markdown for requirements"
✓ Patch da0b844 updated to revision 47019eff7014c1443ffd3802399c1e09196c353b
To compare against your previous revision c6894b5, run:

   git range-diff 4c66f0e[..] 8083d4c[..] 6947d0d[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/da0b844 -> patches/da0b8447cf370a528b6c4a51ff9255eadd726edf
```

Great, all fixed up, lets accept and merge the code.

```
$ rad patch review da0b844 --revision 47019ef --accept
✓ Patch da0b844 accepted
✓ Synced with 1 seed(s)
$ git checkout master
Your branch is up to date with 'rad/master'.
$ git merge patch/da0b844
Updating 4c66f0e..6947d0d
Fast-forward
 README.md       | 0
 REQUIREMENTS.md | 0
 2 files changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
 create mode 100644 REQUIREMENTS.md
```
``` (stderr)
$ git push rad master
✓ Patch da0b8447cf370a528b6c4a51ff9255eadd726edf merged at revision 47019ef
✓ Canonical reference refs/heads/master updated to target commit 6947d0dc43b7e5f7902bbe21550f1dfc1e54b205
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..6947d0d  master -> master
```

The patch is now merged and closed :).

```
$ rad patch show da0b844
╭──────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                               │
│ Patch    da0b8447cf370a528b6c4a51ff9255eadd726edf                │
│ Author   bob z6Mkt67…v4N1tRk                                     │
│ Head     8083d4cbb2b297ade6a12962f8ddc118c3900dcf                │
│ Base     [..                                          ]          │
│ Target   master                                                  │
│ Commits  ahead 0, behind 1                                       │
│ Status   merged                                                  │
│                                                                  │
│ See details.                                                     │
├──────────────────────────────────────────────────────────────────┤
│ 8083d4c Add README, just for the fun                             │
│ 602ba44 Define power requirements                                │
├──────────────────────────────────────────────────────────────────┤
│ ● Revision da0b844 @ [..   ]..602ba44 by bob z6Mkt67…v4N1tRk now │
│ ↑ Revision c6894b5 @ [..   ]..8083d4c by bob z6Mkt67…v4N1tRk now │
│ ↑ Revision 47019ef @ [..   ]..6947d0d by alice (you) now         │
│   └─ ✓ accepted                       by alice (you) now         │
│   └─ ✓ merged                         by alice (you)             │
╰──────────────────────────────────────────────────────────────────╯
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
✓ Synced with 1 seed(s)
```

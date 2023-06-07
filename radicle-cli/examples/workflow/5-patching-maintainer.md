Back to being the project maintainer.

Changes have been proposed by another person (or peer) via a radicle patch.  To follow changes by another, we must 'track' them.

```
$ rad track did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias bob
✓ Tracking policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
```

Additionally, we need to add a new 'git remote' to our working copy for the
peer.  Upcoming versions of radicle will not require this step.

```
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Remote bob added with rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

``` (stderr)
$ git fetch bob
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      master     -> bob/master
 * [new branch]      patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c -> bob/patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c
```

The contributor's changes are now visible to us.

```
$ git branch -r
  bob/master
  bob/patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c
  rad/master
$ rad patch show 50e29a1
╭──────────────────────────────────────────────────────────────────────────────╮
│ Title    Define power requirements                                           │
│ Patch    50e29a111972f3b7d2123c5057de5bdf09bc7b1c                            │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk            │
│ Head     27857ec9eb04c69cacab516e8bf4b5fd36090f66                            │
│ Commits  ahead 2, behind 0                                                   │
│ Status   open                                                                │
│                                                                              │
│ See details.                                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                         │
│ 3e674d1 Define power requirements                                            │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● opened by bob (z6Mkt67…v4N1tRk) [   ...    ]                               │
│ ↑ updated to 3530243d46a2e7a8e4eac7afcbb17cc7c56b3d29 (27857ec) [   ...    ] │
╰──────────────────────────────────────────────────────────────────────────────╯
```

Wait! There's a mistake.  The REQUIREMENTS should be a markdown file.  Let's
quickly update the patch before incorporating the changes.  Updating it this
way will tell others about the corrections we needed before merging the
changes.

```
$ git checkout patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c
branch 'patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c' set up to track 'bob/patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c'.
$ git mv REQUIREMENTS REQUIREMENTS.md
$ git commit --fixup HEAD~
[patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c f6484e0] fixup! Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 rename REQUIREMENTS => REQUIREMENTS.md (100%)
$ rad patch update --message "Define power requirements" --message "See details." 50e29a111972f3b7d2123c5057de5bdf09bc7b1c
Updating 27857ec -> f6484e0
1 commit(s) ahead, 0 commit(s) behind
✓ Patch updated to revision a3405e8e174d9660fead6eea1dea5cdd2b728488
```

Great, all fixed up, lets merge the code.

```
$ git checkout master
Your branch is up to date with 'rad/master'.
$ git merge patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c
Updating f2de534..f6484e0
Fast-forward
 README.md       | 0
 REQUIREMENTS.md | 0
 2 files changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
 create mode 100644 REQUIREMENTS.md
$ git push rad master
```

The patch is now merged and closed :).

```
$ rad patch show 50e29a1
╭──────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                          │
│ Patch     50e29a111972f3b7d2123c5057de5bdf09bc7b1c                           │
│ Author    did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk           │
│ Head      f6484e0f43e48a8983b9b39bf9bd4cd889f1d520                           │
│ Branches  master, patches/50e29a111972f3b7d2123c5057de5bdf09bc7b1c           │
│ Commits   up to date                                                         │
│ Status    merged                                                             │
│                                                                              │
│ See details.                                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ f6484e0 fixup! Define power requirements                                     │
│ 27857ec Add README, just for the fun                                         │
│ 3e674d1 Define power requirements                                            │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● opened by bob (z6Mkt67…v4N1tRk) [   ...    ]                               │
│ ↑ updated to 3530243d46a2e7a8e4eac7afcbb17cc7c56b3d29 (27857ec) [   ...    ] │
│ ↑ updated to a3405e8e174d9660fead6eea1dea5cdd2b728488 (f6484e0) [   ...    ] │
│ ✓ merged by (you) (z6MknSL…StBU8Vi) [   ...    ]                             │
╰──────────────────────────────────────────────────────────────────────────────╯
```

To publish our new state to the network, we simply push:

```
$ git push
```

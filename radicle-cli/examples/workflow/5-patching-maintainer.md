Back to being the project maintainer.

Changes have been proposed by another person (or peer) via a radicle patch.  To follow changes by another, we must 'track' them.

```
$ rad track did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias bob
✓ Tracking policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
! Warning: fetch after track is not yet supported
$ rad fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
```

Additionally, we need to add a new 'git remote' to our working copy for the
peer.  Upcoming versions of radicle will not require this step.

```
$ git remote add bob rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git fetch bob
```

The contributor's changes are now visible to us.

```
$ git branch -r
  bob/flux-capacitor-power
  bob/master
  rad/master
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────╮
│ Define power requirements a07ef77 R1 27857ec ahead 2, behind 0                    │
├───────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk [    ...    ]│
│ ↑ updated to 11483929d8714a92992229f65433e06288f3b760 (27857ec) [         ...    ]│
╰───────────────────────────────────────────────────────────────────────────────────╯
```

Wait! There's a mistake.  The REQUIREMENTS should be a markdown file.  Let's
quickly update the patch before incorporating the changes.  Updating it this
way will tell others about the corrections we needed before merging the
changes.

```
$ git checkout flux-capacitor-power
branch 'flux-capacitor-power' set up to track 'bob/flux-capacitor-power'.
$ git mv REQUIREMENTS REQUIREMENTS.md
$ git commit --fixup HEAD~
[flux-capacitor-power f6484e0] fixup! Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 rename REQUIREMENTS => REQUIREMENTS.md (100%)
$ rad patch update --message "Define power requirements" --message "See details." a07ef7743a32a2e902672ea3526d1db6ee08108a
Updating 27857ec -> f6484e0
1 commit(s) ahead, 0 commit(s) behind
✓ Patch updated to revision 0795d619232479e910f95bb9c873ee1ec305c43c
```

Great, all fixed up, lets merge the code.

```
$ git checkout master
Your branch is up to date with 'rad/master'.
$ rad merge 0795d619232479e910f95bb9c873ee1ec305c43c
Merging a07ef77 R2 (f6484e0) by z6Mkt67…v4N1tRk into master (f2de534) via fast-forward...
Running `git merge --ff-only f6484e0f43e48a8983b9b39bf9bd4cd889f1d520`...
Updating f2de534..f6484e0
Fast-forward
 README.md       | 0
 REQUIREMENTS.md | 0
 2 files changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
 create mode 100644 REQUIREMENTS.md
✓ Updated master f2de534 -> f6484e0 via fast-forward
✓ Patch state updated, use `rad push` to publish
```

The patch is now merged and closed :).

```
$ rad patch --merged
╭───────────────────────────────────────────────────────────────────────────────────────────────╮
│ Define power requirements a07ef77 R2 f6484e0 (flux-capacitor-power, master) ahead 3, behind 0 │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk [            ...        ]│
│ ↑ updated to 11483929d8714a92992229f65433e06288f3b760 (27857ec) [                 ...        ]│
│ ↑ updated to 0795d619232479e910f95bb9c873ee1ec305c43c (f6484e0) [                 ...        ]│
│ ✓ merged by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [      ...        ]│
╰───────────────────────────────────────────────────────────────────────────────────────────────╯
```

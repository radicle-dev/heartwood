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
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Define power requirements a07ef7743a32a2e902672ea3526d1db6ee08108a R1 27857ec ahead 2, behind 0 │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk 3 months ago               │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
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

🌱 Updating patch for heartwood

✓ Pushing HEAD to storage...
✓ Analyzing remotes...

a07ef7743a3 R1 (27857ec) -> R2 (f6484e0)
1 commit(s) ahead, 0 commit(s) behind


✓ Patch a07ef7743a32a2e902672ea3526d1db6ee08108a updated 🌱

```

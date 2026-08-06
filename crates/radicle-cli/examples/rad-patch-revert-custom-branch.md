# Reverting a patch on a custom canonical branch

First, we update the identity document to add a canonical reference rule for a new `accepted` branch.

```
$ rad id -q update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": "delegates" } }'
[..]
```

Now, let's create the `accepted` branch and push it:

``` (stderr)
$ git checkout -b accepted
Switched to a new branch 'accepted'
```
```
$ git commit --allow-empty -m "Initialize accepted branch"
[accepted [..]] Initialize accepted branch
```
``` (stderr)
$ git push rad accepted
✓ Canonical reference refs/heads/accepted updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      accepted -> accepted
```

Next, we create a feature branch and open a patch targeting `accepted`.

``` (stderr)
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ touch FEATURE.md
$ git add FEATURE.md
```
```
$ git commit -m "Add new feature"
[feature/1 [..]] Add new feature
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 FEATURE.md
```
``` (stderr)
$ git push -o patch.message="Add new feature" -o patch.target="refs/heads/accepted" rad HEAD:refs/patches
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Alice merges the feature into the `accepted` branch and pushes it.

``` (stderr)
$ git checkout accepted
Switched to branch 'accepted'
```
```
$ git merge feature/1
Updating [..]
Fast-forward
 FEATURE.md | 0
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 FEATURE.md
```
``` (stderr)
$ git push rad accepted
✓ Patch [..] merged
✓ Canonical reference refs/heads/accepted updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   [..]..[..]  accepted -> accepted
```

We verify the patch is merged.

```
$ rad patch list --merged
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ✓  [..   ]  Add new feature  alice   (you)  -        [..   ]  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

## Revert isolation (Force-pushing the wrong branch)

Now, Alice switches to `master`, resets it to drop a commit, and force pushes. This simulates a scenario where commits are
dropped from a branch *other* than the patch's destination.

``` (stderr)
$ git checkout master
Switched to branch 'master'
```
```
$ git reset --hard HEAD~1
HEAD is now at [..] Initial commit
```
``` (stderr)
$ git push rad master --force
✓ Canonical reference refs/heads/master updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + f2de534...08c788d master -> master (forced update)
```

Because the patch was merged into `accepted`, dropping commits on `master` should not revert the patch. It should remain
merged.

```
$ rad patch list --merged
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ✓  [..   ]  Add new feature  alice   (you)  -        [..   ]  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

## Reverting the patch (Force-pushing the correct branch)

Now, Alice realizes she made a mistake and resets the `accepted` branch, dropping the merge commit, and force pushes.

``` (stderr)
$ git checkout accepted
Switched to branch 'accepted'
```
```
$ git reset --hard HEAD~1
HEAD is now at [..] Initialize accepted branch
```
``` (stderr)
$ git push rad accepted --force
! Patch [..] reverted at revision [..]
✓ Canonical reference refs/heads/accepted updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 27acd3b...f9a3b89 accepted -> accepted (forced update)
```

The patch should now be open again.

```
$ rad patch list --merged
Nothing to show.
```
```
$ rad patch list --open
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ●  [..   ]  Add new feature  alice   (you)  -        [..   ]  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

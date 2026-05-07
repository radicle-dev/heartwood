# Merging patches has a strict destination

First, we update the identity document to add a canonical reference rule for a new `accepted` branch.

```
$ rad id update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": "delegates" } }' -q
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

Next, we create a feature branch and open a patch. We do *not* specify a destination, so it defaults to `master`.

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
$ git push -o patch.message="Add new feature" rad HEAD:refs/patches
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Now, Alice merges the feature into the `accepted` branch instead of `master`.

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
✓ Canonical reference refs/heads/accepted updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   [..]..[..]  accepted -> accepted
```

Because the patch was implicitly targeted for `master`, pushing the commit to `accepted` should not mark it as merged. It should remain open.

```
$ rad patch list --merged
Nothing to show.
```
```
$ rad patch list --open
╭───────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated │
├───────────────────────────────────────────────────────────────────────────────┤
│ ●  [..]  Add new feature  alice   (you)  -        [..]  +0  -0  now     │
╰───────────────────────────────────────────────────────────────────────────────╯
```

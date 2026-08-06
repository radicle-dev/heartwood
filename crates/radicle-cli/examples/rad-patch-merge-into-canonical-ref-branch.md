# Merging patches into non-default canonical branches

First, we update the identity document to add a canonical reference rule for a new `accepted` branch, allowing delegates to merge into it.

```
$ rad id -q update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": "delegates" } }'
[..]
```

Now, let's create the `accepted` branch and push it to the repository so it becomes a tracked canonical reference:

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

Next, we create a feature branch and open a patch. We use the `patch.target` push option to explicitly state that this patch is intended for `refs/heads/accepted` rather than the default branch (`master`).

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

We can verify the patch is open:

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ●  [..   ]  Add new feature  alice   (you)  -        [..   ]  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

Now, Alice merges the feature into the `accepted` branch and pushes it. Because `accepted` is a valid canonical reference and Alice is a delegate, the remote helper should detect the merge and update the patch status.

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

Finally, we verify that the patch has been successfully marked as merged, even though it wasn't merged into the default branch.

```
$ rad patch list --merged
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ✓  [..   ]  Add new feature  alice   (you)  -        [..   ]  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

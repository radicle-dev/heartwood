# Merging patches into the default branch

We create a feature branch and open a patch without specifying a target branch.

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

Now, Alice merges the feature into the `master` branch and pushes it.

``` (stderr)
$ git checkout master
Switched to branch 'master'
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
$ git push rad master
✓ Patch [..] merged
✓ Canonical reference refs/heads/master updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   [..]..[..]  master -> master
```

Finally, we verify that the patch has been successfully marked as merged.

```
$ rad patch list --merged
╭───────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated │
├───────────────────────────────────────────────────────────────────────────────┤
│ ✓  [..]  Add new feature  alice   (you)  -        [..]  +0  -0  now     │
╰───────────────────────────────────────────────────────────────────────────────╯
```

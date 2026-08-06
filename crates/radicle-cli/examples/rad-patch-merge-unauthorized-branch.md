# Merging patches into an unauthorized branch

First, we update the identity document to add a canonical reference rule for a new `accepted` branch, but we only allow Alice to merge into it.

``` ~alice
$ rad id -q update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": ["did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"] } }'
[..]
```

Now, let's create the `accepted` branch and push it:

``` ~alice (stderr)
$ git checkout -b accepted
Switched to a new branch 'accepted'
```
``` ~alice
$ git commit --allow-empty -m "Initialize accepted branch"
[accepted [..]] Initialize accepted branch
```
``` ~alice (stderr)
$ git push rad accepted
✓ Canonical reference refs/heads/accepted updated to target commit [..]
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      accepted -> accepted
```

Next, Bob clones the repository and opens a patch targeting `accepted`.

``` ~bob
$ rad clone rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
✓ Repository successfully cloned under [..]
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
```
``` ~bob (stderr)
$ cd heartwood
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ touch FEATURE.md
$ git add FEATURE.md
```
``` ~bob
$ git commit -m "Add new feature"
[feature/1 [..]] Add new feature
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 FEATURE.md
```
``` ~bob (stderr)
$ git push -o patch.message="Add new feature" -o patch.target="refs/heads/accepted" rad HEAD:refs/patches
✓ Patch [..] opened
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

Now, Bob tries to merge the feature into the `accepted` branch and push it.

``` ~bob (stderr)
$ git checkout -t rad/accepted
Switched to a new branch 'accepted'
```
``` ~bob
$ git merge feature/1
Merge made by the 'ort' strategy.
 FEATURE.md | 0
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 FEATURE.md
```
``` ~bob (stderr)
$ git push rad accepted
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      accepted -> accepted
```

Notice that the canonical reference was *not* updated because Bob is not authorized. The patch should remain open.

``` ~bob
$ rad patch list --merged
Nothing to show.
```
``` ~bob
$ rad patch list --open
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ●  [..   ]  Add new feature  bob     (you)  -        [..   ]  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

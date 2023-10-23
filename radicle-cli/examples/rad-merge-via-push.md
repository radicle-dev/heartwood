Let's start by creating two patches.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch f6c96cca58521d6dbb6cd4e6b7124342b9a86945 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -q -m "Second change"
$ git push rad HEAD:refs/patches
✓ Patch 3b8203713e2945a6c46b238e6a432bd2711d3ccf opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/3b8203713e2945a6c46b238e6a432bd2711d3ccf
  rad/patches/f6c96cca58521d6dbb6cd4e6b7124342b9a86945
```

And some remote refs:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── 2317f74de0494c489a233ca6f29f2b8bff6d4f15
    │   └── xyz.radicle.patch
    │       ├── 3b8203713e2945a6c46b238e6a432bd2711d3ccf
    │       └── f6c96cca58521d6dbb6cd4e6b7124342b9a86945
    ├── heads
    │   ├── master
    │   └── patches
    │       ├── 3b8203713e2945a6c46b238e6a432bd2711d3ccf
    │       └── f6c96cca58521d6dbb6cd4e6b7124342b9a86945
    └── rad
        ├── id
        └── sigrefs
```

Then let's merge the changes into `master`.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout master
Switched to branch 'master'
$ git merge feature/1
$ git merge feature/2
```

When we push to `rad/master`, we automatically merge the patches:

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad master
✓ Patch 3b8203713e2945a6c46b238e6a432bd2711d3ccf merged
✓ Patch f6c96cca58521d6dbb6cd4e6b7124342b9a86945 merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..d6399c7  master -> master
```
```
$ rad patch --merged
╭────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author         Head     +   -   Updated │
├────────────────────────────────────────────────────────────────────┤
│ ✔  [ ... ]  Second change  alice   (you)  daf349f  +0  -0  now     │
│ ✔  [ ... ]  First change   alice   (you)  20aa5dd  +0  -0  now     │
╰────────────────────────────────────────────────────────────────────╯
```

We can verify that the remote tracking branches were also deleted:

```
$ git branch -r
  rad/master
```

And so were the remote branches:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── 2317f74de0494c489a233ca6f29f2b8bff6d4f15
    │   └── xyz.radicle.patch
    │       ├── 3b8203713e2945a6c46b238e6a432bd2711d3ccf
    │       └── f6c96cca58521d6dbb6cd4e6b7124342b9a86945
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
```

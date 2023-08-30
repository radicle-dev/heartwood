Let's start by creating two patches.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 143bb0c962561b09e86478a53ba346b5ff934335 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -q -m "Second change"
$ git push rad HEAD:refs/patches
✓ Patch 5d0e608aa35af59f769e9d6a2c0227ea60ae2740 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/143bb0c962561b09e86478a53ba346b5ff934335
  rad/patches/5d0e608aa35af59f769e9d6a2c0227ea60ae2740
```

And some remote refs:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.patch
    │       ├── 143bb0c962561b09e86478a53ba346b5ff934335
    │       └── 5d0e608aa35af59f769e9d6a2c0227ea60ae2740
    ├── heads
    │   ├── master
    │   └── patches
    │       ├── 143bb0c962561b09e86478a53ba346b5ff934335
    │       └── 5d0e608aa35af59f769e9d6a2c0227ea60ae2740
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
✓ Patch 143bb0c962561b09e86478a53ba346b5ff934335 merged
✓ Patch 5d0e608aa35af59f769e9d6a2c0227ea60ae2740 merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..d6399c7  master -> master
```
```
$ rad patch --merged
╭─────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author         Head     +   -   Updated      │
├─────────────────────────────────────────────────────────────────────────┤
│ ✔  143bb0c  First change   alice   (you)  20aa5dd  +0  -0  [    ...   ] │
│ ✔  5d0e608  Second change  alice   (you)  daf349f  +0  -0  [    ...   ] │
╰─────────────────────────────────────────────────────────────────────────╯
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
    │   └── xyz.radicle.patch
    │       ├── 143bb0c962561b09e86478a53ba346b5ff934335
    │       └── 5d0e608aa35af59f769e9d6a2c0227ea60ae2740
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
```

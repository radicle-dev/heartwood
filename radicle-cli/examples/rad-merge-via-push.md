Let's start by creating two patches.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 696ec5508494692899337afe6713fe1796d0315c opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -q -m "Second change"
$ git push rad HEAD:refs/patches
✓ Patch 356f73863a8920455ff6e77cd9c805d68910551b opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/356f73863a8920455ff6e77cd9c805d68910551b
  rad/patches/696ec5508494692899337afe6713fe1796d0315c
```

And some remote refs:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── 0656c217f917c3e06234771e9ecae53aba5e173e
    │   └── xyz.radicle.patch
    │       ├── 356f73863a8920455ff6e77cd9c805d68910551b
    │       └── 696ec5508494692899337afe6713fe1796d0315c
    ├── heads
    │   ├── master
    │   └── patches
    │       ├── 356f73863a8920455ff6e77cd9c805d68910551b
    │       └── 696ec5508494692899337afe6713fe1796d0315c
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
✓ Patch 356f73863a8920455ff6e77cd9c805d68910551b merged
✓ Patch 696ec5508494692899337afe6713fe1796d0315c merged
✓ Canonical head updated to d6399c71702b40bae00825b3c444478d06b4e91c
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
$ rad patch show 356f73863a8920455ff6e77cd9c805d68910551b
╭────────────────────────────────────────────────────────────────╮
│ Title     Second change                                        │
│ Patch     356f73863a8920455ff6e77cd9c805d68910551b             │
│ Author    alice (you)                                          │
│ Head      daf349ff76bedf48c5f292290b682ee7be0683cf             │
│ Branches  feature/2                                            │
│ Commits   ahead 0, behind 2                                    │
│ Status    merged                                               │
├────────────────────────────────────────────────────────────────┤
│ daf349f Second change                                          │
├────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (daf349f) now                          │
│   └─ ✓ merged by alice (you) at revision 356f738 (d6399c7) now │
╰────────────────────────────────────────────────────────────────╯
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
    │   │   └── 0656c217f917c3e06234771e9ecae53aba5e173e
    │   └── xyz.radicle.patch
    │       ├── 356f73863a8920455ff6e77cd9c805d68910551b
    │       └── 696ec5508494692899337afe6713fe1796d0315c
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
```

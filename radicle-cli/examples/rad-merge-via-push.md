Let's start by creating two patches.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch b082560898736233790dedff7b1a725b18614480 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -q -m "Second change"
$ git push rad HEAD:refs/patches
✓ Patch 80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd
  rad/patches/b082560898736233790dedff7b1a725b18614480
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
    │       ├── 80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd
    │       └── b082560898736233790dedff7b1a725b18614480
    ├── heads
    │   ├── master
    │   └── patches
    │       ├── 80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd
    │       └── b082560898736233790dedff7b1a725b18614480
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
✓ Patch 80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd merged
✓ Patch b082560898736233790dedff7b1a725b18614480 merged
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
$ rad patch show 80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd
╭────────────────────────────────────────────────────────────────╮
│ Title     Second change                                        │
│ Patch     80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd             │
│ Author    alice (you)                                          │
│ Head      daf349ff76bedf48c5f292290b682ee7be0683cf             │
│ Branches  feature/2                                            │
│ Commits   ahead 0, behind 2                                    │
│ Status    merged                                               │
├────────────────────────────────────────────────────────────────┤
│ daf349f Second change                                          │
├────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (daf349f) now                          │
│   └─ ✓ merged by alice (you) at revision 80fe6a0 (d6399c7) now │
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
    │       ├── 80fe6a0c283d7209f8839c79bc90ff9ecd9fdedd
    │       └── b082560898736233790dedff7b1a725b18614480
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
```

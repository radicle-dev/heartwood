1
Let's start by creating two patches.

```
$ git checkout -b feature/1 -q
$ git commit --allow-empty -m "First change"
[feature/1 20aa5dd] First change
```
``` (stderr) RAD_SOCKET=/dev/null
$ git push rad HEAD:refs/patches
✓ Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
```
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -m "Second change"
[feature/2 daf349f] Second change
```
``` (stderr) RAD_SOCKET=/dev/null
$ git push rad HEAD:refs/patches
✓ Patch 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/09a3de4ac2c4d012c4a9c84c0cb306a066a0b084
  rad/patches/0e8cc60585b6bb6a1236dc9958bf09883ecba9f3
```

And some remote refs:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── eeb8b44890570ccf85db7f3cb2a475100a27408a
    │   └── xyz.radicle.patch
    │       ├── 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084
    │       └── 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3
    ├── heads
    │   ├── master
    │   └── patches
    │       ├── 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084
    │       └── 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3
    └── rad
        ├── id
        ├── root
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
✓ Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 merged
✓ Patch 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3 merged
✓ Canonical head for refs/heads/master updated to d6399c71702b40bae00825b3c444478d06b4e91c
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..d6399c7  master -> master
```
```
$ rad patch --merged
╭─────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────┤
│ ✔  [ ... ]  First change   alice   (you)  -        20aa5dd  +0  -0  now     │
│ ✔  [ ... ]  Second change  alice   (you)  -        daf349f  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────╯
$ rad patch show 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3
╭────────────────────────────────────────────────────────────────╮
│ Title     Second change                                        │
│ Patch     0e8cc60585b6bb6a1236dc9958bf09883ecba9f3             │
│ Author    alice (you)                                          │
│ Head      daf349ff76bedf48c5f292290b682ee7be0683cf             │
│ Branches  feature/2                                            │
│ Commits   ahead 0, behind 2                                    │
│ Status    merged                                               │
├────────────────────────────────────────────────────────────────┤
│ daf349f Second change                                          │
├────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (daf349f) now                          │
│   └─ ✓ merged by alice (you) at revision 0e8cc60 (daf349f) now │
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
    │   │   └── eeb8b44890570ccf85db7f3cb2a475100a27408a
    │   └── xyz.radicle.patch
    │       ├── 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084
    │       └── 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3
    ├── heads
    │   └── master
    └── rad
        ├── id
        ├── root
        └── sigrefs
```

Finally, let's check that we can revert the second patch without affecting
the first patch, even though they were pushed together.

``` (stderr) RAD_SOCKET=/dev/null
$ git reset --hard HEAD^
$ git push -f rad
! Patch 0e8cc60585b6bb6a1236dc9958bf09883ecba9f3 reverted at revision 0e8cc60
✓ Canonical head for refs/heads/master updated to 20aa5dde6210796c3a2f04079b42316a31d02689
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + d6399c7...20aa5dd master -> master (forced update)
```
```
$ rad patch --all
╭─────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────┤
│ ✔  09a3de4  First change   alice   (you)  -        20aa5dd  +0  -0  now     │
│ ●  0e8cc60  Second change  alice   (you)  -        daf349f  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────╯
```

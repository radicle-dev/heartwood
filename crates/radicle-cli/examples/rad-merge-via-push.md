Let's start by creating two patches.

```
$ git checkout -b feature/1 -q
$ git commit --allow-empty -m "First change"
[feature/1 f708282] First change
```
``` (stderr) RAD_SOCKET=/dev/null
$ git push rad HEAD:refs/patches
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
```
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -m "Second change"
[feature/2 549bbd7] Second change
```
``` (stderr) RAD_SOCKET=/dev/null
$ git push rad HEAD:refs/patches
✓ Patch 4410f6bc82843c083850920ffee1977fcd6645c9 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/4410f6bc82843c083850920ffee1977fcd6645c9
  rad/patches/91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
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
    │       ├── 4410f6bc82843c083850920ffee1977fcd6645c9
    │       └── 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
    ├── heads
    │   ├── master
    │   └── patches
    │       ├── 4410f6bc82843c083850920ffee1977fcd6645c9
    │       └── 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
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
✓ Patch 4410f6bc82843c083850920ffee1977fcd6645c9 merged
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be merged
✓ Canonical reference refs/heads/master updated to target commit 93b0c9bcd127803118f2fc91d784eb177318e0ab
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..93b0c9b  master -> master
```
```
$ rad patch --merged
╭─────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ✓  [ ... ]  Second change  alice   (you)  -        549bbd7  +0  -0  now             │
│ ✓  [ ... ]  First change   alice   (you)  -        f708282  +0  -0  now             │
╰─────────────────────────────────────────────────────────────────────────────────────╯
$ rad patch show 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
╭──────────────────────────────────────────────────────────╮
│ Title     First change                                   │
│ Patch     91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be       │
│ Author    alice (you)                                    │
│ Head      f708282adae8d6f31eda23c8ecf3120eb99a499b       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   ahead 0, behind 2                              │
│ Status    merged                                         │
├──────────────────────────────────────────────────────────┤
│ f708282 First change                                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 91f94ec @ [..   ]..f708282 by alice (you) now │
│   └─ ✓ merged                         by alice (you)     │
╰──────────────────────────────────────────────────────────╯
$ rad patch show 4410f6bc82843c083850920ffee1977fcd6645c9
╭──────────────────────────────────────────────────────────╮
│ Title     Second change                                  │
│ Patch     4410f6bc82843c083850920ffee1977fcd6645c9       │
│ Author    alice (you)                                    │
│ Head      549bbd75d584d6924545a6672fd7397e5fb3eebb       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/2                                      │
│ Commits   ahead 0, behind 2                              │
│ Status    merged                                         │
├──────────────────────────────────────────────────────────┤
│ 549bbd7 Second change                                    │
├──────────────────────────────────────────────────────────┤
│ ● Revision 4410f6b @ [..   ]..549bbd7 by alice (you) now │
│   └─ ✓ merged                         by alice (you)     │
╰──────────────────────────────────────────────────────────╯
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
    │       ├── 4410f6bc82843c083850920ffee1977fcd6645c9
    │       └── 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
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
! Patch 4410f6bc82843c083850920ffee1977fcd6645c9 reverted at revision 4410f6b
✓ Canonical reference refs/heads/master updated to target commit f708282adae8d6f31eda23c8ecf3120eb99a499b
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 93b0c9b...f708282 master -> master (forced update)
```
```
$ rad patch --all
╭─────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ●  4410f6b  Second change  alice   (you)  -        549bbd7  +0  -0  now             │
│ ✓  91f94ec  First change   alice   (you)  -        f708282  +0  -0  now             │
╰─────────────────────────────────────────────────────────────────────────────────────╯
```

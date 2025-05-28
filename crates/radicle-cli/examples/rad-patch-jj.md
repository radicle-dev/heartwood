The scenario in this file is a variation of the one in `rad-patch.md`,
but uses Jujutsu.

```
$ touch REQUIREMENTS
$ jj describe --message "Define power requirements"
$ jj status
Working copy changes:
A REQUIREMENTS
Working copy  (@) : lvxkkpmk a6ea7b72 Define power requirements
Parent commit (@-): xpnzuzwn f2de534b master master@rad | Second commit
```

```
$ jj new
```

Just making sure that Git sees the Change ID…

```
$ git cat-file commit a6ea7b72
tree [..]
parent f2de534b[..]
author Test User <test.user@example.com> 981147906 +0700
committer Test User <test.user@example.com> 981147906 +0700
change-id lvxkkpmk[..]

Define power requirements
```

As of 2025-05 we can't use `jj` to do push with options directly, see:

 - <https://github.com/jj-vcs/jj/issues/4075>
 - <https://github.com/jj-vcs/jj/pull/2098>

However, since we initialized Jujutusu to colocate with Git, we can just use
Git to push.

``` (stderr)
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 1e31055ed3c41a48f2a71ba5317feb863b089700 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  1e31055  Define power requirements  alice   (you)  -        a6ea7b7  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

Let's also create a bookmark for it.

```
$ jj bookmark create flux-capacitor-power
```

```
$ rad patch show 1e31055 -p
╭───────────────────────────────────────────────────╮
│ Title    Define power requirements                │
│ Patch    1e31055[..                             ] │
│ Author   alice (you)                              │
│ Head     a6ea7b7[..                             ] │
│ Base     f2de534[..                             ] │
│ Commits  ahead 1, behind 0                        │
│ Status   open                                     │
│                                                   │
│ See details.                                      │
├───────────────────────────────────────────────────┤
│ a6ea7b7 Define power requirements                 │
├───────────────────────────────────────────────────┤
│ ● Revision 1e31055 @ a6ea7b7 by alice (you) now   │
╰───────────────────────────────────────────────────╯

commit a6ea7b7[..]
Author: Test User <test.user@example.com>
Date:   Sat Feb 3 04:05:06 2001 +0700

    Define power requirements

diff --git a/REQUIREMENTS b/REQUIREMENTS
new file mode 100644
index 0000000..e69de29

```
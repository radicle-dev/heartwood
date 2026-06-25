The scenario in this file is a variation of the one in `rad-patch.md`,
but uses Jujutsu.

```
$ touch REQUIREMENTS
$ jj describe --message "Define power requirements"
$ jj status
Working copy changes:
A REQUIREMENTS
Working copy  (@) : lvxkkpmk f54b4670 Define power requirements
Parent commit (@-): lvqposkz 4c66f0e9 master master@rad | Second commit
```

```
$ jj new
```

Just making sure that Git sees the Change ID…

```
$ git cat-file commit f54b4670
tree [..]
parent 4c66f0e9[..]
author Test User <test.user@example.com> 981147906 +0700
committer Test User <test.user@example.com> 981147906 +0700
change-id lvxkkpmk[..]

Define power requirements
```

As of 2025-05 we can't use `jj` to do push with options directly, see:

 - <https://github.com/jj-vcs/jj/issues/4075>
 - <https://github.com/jj-vcs/jj/pull/2098>

However, since we initialized Jujutsu to colocate with Git, we can just use
Git to push.

``` (stderr)
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch e0c8a16d35ce5c7e3de4fcb4103b1a103a286c8b opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  e0c8a16  Define power requirements  alice   (you)  -        f54b467  +0  -0  now             │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Let's also create a bookmark for it.

```
$ jj bookmark create flux-capacitor-power
```

```
$ rad patch show e0c8a16 -p
╭──────────────────────────────────────────────────────────╮
│ Title    Define power requirements                       │
│ Patch    e0c8a16[..                             ]        │
│ Author   alice (you)                                     │
│ Head     f54b467[..                             ]        │
│ Base     4c66f0e[..                             ]        │
│ Target   master                                          │
│ Commits  ahead 1, behind 0                               │
│ Status   open                                            │
│                                                          │
│ See details.                                             │
├──────────────────────────────────────────────────────────┤
│ f54b467 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision e0c8a16 @ [..   ]..f54b467 by alice (you) now │
╰──────────────────────────────────────────────────────────╯

commit f54b467[..]
Author: Test User <test.user@example.com>
Date:   Sat Feb 3 04:05:06 2001 +0700

    Define power requirements

diff --git a/REQUIREMENTS b/REQUIREMENTS
new file mode 100644
index 0000000..e69de29

```

Let's start by creating two patches.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 0ec956c94256fa101db4c32956ce195a1aa0edf2 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/2 -q master
$ git commit --allow-empty -q -m "Second change"
$ git push rad HEAD:refs/patches
✓ Patch 928d76e22ef98a8406f2e4e4bcc8878533bbdfe0 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

This creates some remote tracking branches for us:

```
$ git branch -r
  rad/master
  rad/patches/0ec956c94256fa101db4c32956ce195a1aa0edf2
  rad/patches/928d76e22ef98a8406f2e4e4bcc8878533bbdfe0
```

And some remote refs:

```
$ rad inspect --refs
.
`-- z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
    `-- refs
        |-- cobs
        |   `-- xyz.radicle.patch
        |       |-- 0ec956c94256fa101db4c32956ce195a1aa0edf2
        |       `-- 928d76e22ef98a8406f2e4e4bcc8878533bbdfe0
        |-- heads
        |   |-- master
        |   `-- patches
        |       |-- 0ec956c94256fa101db4c32956ce195a1aa0edf2
        |       `-- 928d76e22ef98a8406f2e4e4bcc8878533bbdfe0
        `-- rad
            |-- id
            `-- sigrefs
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
✓ Patch 0ec956c94256fa101db4c32956ce195a1aa0edf2 merged
✓ Patch 928d76e22ef98a8406f2e4e4bcc8878533bbdfe0 merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..d6399c7  master -> master
```
```
$ rad patch --merged
╭────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author                        Head     +   -   Updated      │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ ✔  0ec956c  First change   z6MknSL…StBU8Vi  alice (you)  20aa5dd  +0  -0  [   ...    ] │
│ ✔  928d76e  Second change  z6MknSL…StBU8Vi  alice (you)  daf349f  +0  -0  [   ...    ] │
╰────────────────────────────────────────────────────────────────────────────────────────╯
```

We can verify that the remote tracking branches were also deleted:

```
$ git branch -r
  rad/master
```

And so were the remote branches:

```
$ rad inspect --refs
.
`-- z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
    `-- refs
        |-- cobs
        |   `-- xyz.radicle.patch
        |       |-- 0ec956c94256fa101db4c32956ce195a1aa0edf2
        |       `-- 928d76e22ef98a8406f2e4e4bcc8878533bbdfe0
        |-- heads
        |   `-- master
        `-- rad
            |-- id
            `-- sigrefs
```

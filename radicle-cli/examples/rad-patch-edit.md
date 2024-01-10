If you ever want to change the title and descriptions associated with
a patch and its revisions, we can always use the `rad patch edit`
command.

First off, we'll have to set up a patch and an updated revision:

```
$ git checkout -b changes
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[changes 03c02af] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun" HEAD:refs/patches
✓ Patch 0631b2d1b5cd4ae44e92732ff3929528354a8405 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ touch LICENSE
$ git add LICENSE
$ git commit -v -m "Define the LICENSE"
[changes 8945f61] Define the LICENSE
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

``` (stderr)
$ git push -f -o patch.message="Add License"
✓ Patch 0631b2d updated to revision bbc91fd4c74bfbbb162ec4b97cd8cefe623814d7
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   03c02af..8945f61  changes -> patches/0631b2d1b5cd4ae44e92732ff3929528354a8405
```

Let's look at the patch, to see what it looks like before editing it:

```
$ rad patch show 0631b2d
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                              │
│ Patch     0631b2d1b5cd4ae44e92732ff3929528354a8405                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (03c02af) now                     │
│ ↑ updated to bbc91fd4c74bfbbb162ec4b97cd8cefe623814d7 (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We can change the title and description of the patch itself by using a
multi-line message (using two `--message` options here):

```
$ rad patch edit 0631b2d --message "Add Metadata" --message "Add README & LICENSE" --no-announce
$ rad patch show 0631b2d
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     0631b2d1b5cd4ae44e92732ff3929528354a8405                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add README & LICENSE                                                │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (03c02af) now                     │
│ ↑ updated to bbc91fd4c74bfbbb162ec4b97cd8cefe623814d7 (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

Notice that the `Title` is now `Add Metadata`, and the patch now has a
description `Add README & LICENSE`.

If we want to change a specific revision's description, we can use the
`--revision` option:

```
$ rad patch edit 0631b2d --revision bbc91fd --message "Changes: Adds LICENSE file" --no-announce
$ rad patch show 0631b2d
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     0631b2d1b5cd4ae44e92732ff3929528354a8405                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add README & LICENSE                                                │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (03c02af) now                     │
│ ↑ updated to bbc91fd4c74bfbbb162ec4b97cd8cefe623814d7 (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We can see that this didn't affect the patch's description, but
currently there's no way of seeing a revision's description in the
CLI.

If you ever want to change the title and descriptions associated with
a patch and its revisions, we can always use the `rad patch edit`
command.

First off, we'll have to set up a patch and an updated revision:

```
$ git checkout -b changes
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[changes ad12b3e] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun" HEAD:refs/patches
✓ Patch 564037be20294ec4288f980e8defd394c4572c9a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ touch LICENSE
$ git add LICENSE
$ git commit -v -m "Define the LICENSE"
[changes 098f4c5] Define the LICENSE
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

``` (stderr)
$ git push -f -o patch.message="Add License"
✓ Patch 564037b updated to revision b650ef8d9eb39de4888f2c1a4cd3ee2a3e72f38d
To compare against your previous revision 564037b, run:

   git range-diff 4c66f0e[..] ad12b3e[..] 098f4c5[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   ad12b3e..098f4c5  changes -> patches/564037be20294ec4288f980e8defd394c4572c9a
```

Let's look at the patch, to see what it looks like before editing it:

```
$ rad patch show 564037b
╭──────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                   │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a       │
│ Author    alice (you)                                    │
│ Head      098f4c53591b72ec2e69a7b33f83d04aa92253e5       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  changes                                        │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 098f4c5 Define the LICENSE                               │
│ ad12b3e Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 564037b @ [..   ]..ad12b3e by alice (you) now │
│ ↑ Revision b650ef8 @ [..   ]..098f4c5 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

We can change the title and description of the patch itself by using a
multi-line message (using two `--message` options here):

```
$ rad patch edit 564037b --message "Add Metadata" --message "Add README & LICENSE" --no-announce
$ rad patch show 564037b
╭──────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                   │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a       │
│ Author    alice (you)                                    │
│ Head      098f4c53591b72ec2e69a7b33f83d04aa92253e5       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  changes                                        │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add README & LICENSE                                     │
├──────────────────────────────────────────────────────────┤
│ 098f4c5 Define the LICENSE                               │
│ ad12b3e Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 564037b @ [..   ]..ad12b3e by alice (you) now │
│ ↑ Revision b650ef8 @ [..   ]..098f4c5 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

Notice that the `Title` is now `Add Metadata`, and the patch now has a
description `Add README & LICENSE`.

If we want to change a specific revision's description, we can use the
`--revision` option:

```
$ rad patch edit 564037b --revision b650ef8 --message "Changes: Adds LICENSE file" --no-announce
$ rad patch show 564037b
╭──────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                   │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a       │
│ Author    alice (you)                                    │
│ Head      098f4c53591b72ec2e69a7b33f83d04aa92253e5       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  changes                                        │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add README & LICENSE                                     │
├──────────────────────────────────────────────────────────┤
│ 098f4c5 Define the LICENSE                               │
│ ad12b3e Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 564037b @ [..   ]..ad12b3e by alice (you) now │
│ ↑ Revision b650ef8 @ [..   ]..098f4c5 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

We can see that this didn't affect the patch's description, but
currently there's no way of seeing a revision's description in the
CLI.

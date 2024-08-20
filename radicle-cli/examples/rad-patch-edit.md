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
✓ Patch f699e2299e9ee734758626924df7e15fd9a68553 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
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
✓ Patch f699e22 updated to revision 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f
To compare against your previous revision f699e22, run:

   git range-diff f2de534[..] 03c02af[..] 8945f61[..]

To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   03c02af..8945f61  changes -> patches/f699e2299e9ee734758626924df7e15fd9a68553
```

Let's look at the patch, to see what it looks like before editing it:

```
$ rad patch show f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
│ Author    alice (you)                                               │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We can change the title and description of the patch itself by using a
multi-line message (using two `--message` options here):

```
$ rad patch edit f699e22 --message "Add Metadata" --message "Add README & LICENSE" --no-announce
$ rad patch show f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
│ Author    alice (you)                                               │
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
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

Notice that the `Title` is now `Add Metadata`, and the patch now has a
description `Add README & LICENSE`.

If we want to change a specific revision's description, we can use the
`--revision` option:

```
$ rad patch edit f699e22 --revision 13b5240 --message "Changes: Adds LICENSE file" --no-announce
$ rad patch show f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
│ Author    alice (you)                                               │
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
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We can see that this didn't affect the patch's description, but
currently there's no way of seeing a revision's description in the
CLI.

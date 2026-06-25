Let's explore the `rad patch update` plumbing command. First we create a patch:

``` (stderr)
$ git checkout -q -b feature/1
$ git commit -q -m "Not a real change" --allow-empty
```
``` (stderr)
$ git push rad HEAD:refs/patches
✓ Patch 811ff0dc4eb3ed1858d9747ba1d4736d3d18ad1b opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ rad patch show 811ff0dc4eb3ed1858d9747ba1d4736d3d18ad1b
╭──────────────────────────────────────────────────────────╮
│ Title     Not a real change                              │
│ Patch     811ff0dc4eb3ed1858d9747ba1d4736d3d18ad1b       │
│ Author    alice (you)                                    │
│ Head      6248bc11b44db82e41be5434d0c73433d0840832       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 6248bc1 Not a real change                                │
├──────────────────────────────────────────────────────────┤
│ ● Revision 811ff0d @ [..   ]..6248bc1 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

We can make some changes to the repository:

```
$ git mv README README.md
$ git commit -q -m "Rename readme file"
```

Let's push the changes, but not to the magic ref, that way the push doesn't
update our patch:

```
$ git push rad HEAD:refs/heads/feature/1
```

Now, instead of using `git push` to update the patch, as we normally would,
we run:

```
$ rad patch update 811ff0dc4eb3ed1858d9747ba1d4736d3d18ad1b -m "Updated patch" --no-announce
74f47c085c8c8b982df4ba1d8e695c29778b0509
```

The command outputs the new Revision ID, which we can now see here:

```
$ rad patch show 811ff0dc4eb3ed1858d9747ba1d4736d3d18ad1b
╭──────────────────────────────────────────────────────────╮
│ Title     Not a real change                              │
│ Patch     811ff0dc4eb3ed1858d9747ba1d4736d3d18ad1b       │
│ Author    alice (you)                                    │
│ Head      eece48c32f3b34b741e4f6629448195c76e49f95       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ eece48c Rename readme file                               │
│ 6248bc1 Not a real change                                │
├──────────────────────────────────────────────────────────┤
│ ● Revision 811ff0d @ [..   ]..6248bc1 by alice (you) now │
│ ↑ Revision 74f47c0 @ [..   ]..eece48c by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

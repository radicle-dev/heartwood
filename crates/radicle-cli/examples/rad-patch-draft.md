Let's say we have some changes in a branch:

```
$ git checkout -b cloudhead/draft
$ git commit -a -m "Nothing to see here.." -q --allow-empty
```

To open a patch in draft mode, we use the `--draft` option:

``` (stderr)
$ git push -o patch.draft -o patch.message="Nothing yet" rad HEAD:refs/patches
✓ Patch bc7acface641013e802d51f3e9d06fb4f89418ef drafted
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can confirm it's a draft by running `show`:

```
$ rad patch show bc7acface641013e802d51f3e9d06fb4f89418ef
╭──────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                    │
│ Patch     bc7acface641013e802d51f3e9d06fb4f89418ef       │
│ Author    alice (you)                                    │
│ Head      d33f83145e2181a89f6e236d52c29104e2ff8b26       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  cloudhead/draft                                │
│ Commits   ahead 1, behind 0                              │
│ Status    draft                                          │
├──────────────────────────────────────────────────────────┤
│ d33f831 Nothing to see here..                            │
├──────────────────────────────────────────────────────────┤
│ ● Revision bc7acfa @ [..   ]..d33f831 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

Once the patch is ready for review, we can use the `ready` command:

```
$ rad patch ready bc7acface641013e802d51f3e9d06fb4f89418ef --no-announce
```

```
$ rad patch show bc7acface641013e802d51f3e9d06fb4f89418ef
╭──────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                    │
│ Patch     bc7acface641013e802d51f3e9d06fb4f89418ef       │
│ Author    alice (you)                                    │
│ Head      d33f83145e2181a89f6e236d52c29104e2ff8b26       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  cloudhead/draft                                │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ d33f831 Nothing to see here..                            │
├──────────────────────────────────────────────────────────┤
│ ● Revision bc7acfa @ [..   ]..d33f831 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

If for whatever reason, it needed to go back into draft mode, we could use
the `--undo` flag:

```
$ rad patch ready --undo bc7acface641013e802d51f3e9d06fb4f89418ef --no-announce
$ rad patch show bc7acface641013e802d51f3e9d06fb4f89418ef
╭──────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                    │
│ Patch     bc7acface641013e802d51f3e9d06fb4f89418ef       │
│ Author    alice (you)                                    │
│ Head      d33f83145e2181a89f6e236d52c29104e2ff8b26       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  cloudhead/draft                                │
│ Commits   ahead 1, behind 0                              │
│ Status    draft                                          │
├──────────────────────────────────────────────────────────┤
│ d33f831 Nothing to see here..                            │
├──────────────────────────────────────────────────────────┤
│ ● Revision bc7acfa @ [..   ]..d33f831 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

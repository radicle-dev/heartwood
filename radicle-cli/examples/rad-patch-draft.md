Let's say we have some changes in a branch:

```
$ git checkout -b cloudhead/draft
$ git commit -a -m "Nothing to see here.." -q --allow-empty
```

To open a patch in draft mode, we use the `--draft` option:

``` (stderr)
$ git push -o patch.draft -o patch.message="Nothing yet" rad HEAD:refs/patches
✓ Patch c639a0f9895a0fdf2ba2d04533290937cb6fd2f7 drafted
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can confirm it's a draft by running `show`:

```
$ rad patch show c639a0f9895a0fdf2ba2d04533290937cb6fd2f7
╭────────────────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                              │
│ Patch     c639a0f9895a0fdf2ba2d04533290937cb6fd2f7                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7                 │
│ Branches  cloudhead/draft                                          │
│ Commits   ahead 1, behind 0                                        │
│ Status    draft                                                    │
├────────────────────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                                      │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) [   ...    ]                                     │
╰────────────────────────────────────────────────────────────────────╯
```

Once the patch is ready for review, we can use the `ready` command:

```
$ rad patch ready c639a0f9895a0fdf2ba2d04533290937cb6fd2f7
```

```
$ rad patch show c639a0f9895a0fdf2ba2d04533290937cb6fd2f7
╭────────────────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                              │
│ Patch     c639a0f9895a0fdf2ba2d04533290937cb6fd2f7                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7                 │
│ Branches  cloudhead/draft                                          │
│ Commits   ahead 1, behind 0                                        │
│ Status    open                                                     │
├────────────────────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                                      │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) [   ...    ]                                     │
╰────────────────────────────────────────────────────────────────────╯
```

If for whatever reason, it needed to go back into draft mode, we could use
the `--undo` flag:

```
$ rad patch ready --undo c639a0f9895a0fdf2ba2d04533290937cb6fd2f7
$ rad patch show c639a0f9895a0fdf2ba2d04533290937cb6fd2f7
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                                                   │
│ Patch     c639a0f9895a0fdf2ba2d04533290937cb6fd2f7                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7                                      │
│ Branches  cloudhead/draft                                                               │
│ Commits   ahead 1, behind 0                                                             │
│ Status    draft                                                                         │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                                                           │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) [   ...    ]                                                          │
╰─────────────────────────────────────────────────────────────────────────────────────────╯

Let's say we have some changes in a branch:

```
$ git checkout -b cloudhead/draft
$ git commit -a -m "Nothing to see here.." -q --allow-empty
```

To open a patch in draft mode, we use the `--draft` option:

```
$ rad patch open --draft -m "Nothing yet" --quiet
6d7793b859860db775fd8ff1d18ffb6de2b9ca0e
```

We can confirm it's a draft by running `show`:

```
$ rad patch show 6d7793b859860db775fd8ff1d18ffb6de2b9ca0e
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                                                   │
│ Patch     6d7793b859860db775fd8ff1d18ffb6de2b9ca0e                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7                                      │
│ Branches  cloudhead/draft                                                               │
│ Commits   ahead 1, behind 0                                                             │
│ Status    draft                                                                         │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                                                           │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [   ...    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

Once the patch is ready for review, we can use the `ready` command:

```
$ rad patch ready 6d7793b859860db775fd8ff1d18ffb6de2b9ca0e
```

```
$ rad patch show 6d7793b859860db775fd8ff1d18ffb6de2b9ca0e
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                                                   │
│ Patch     6d7793b859860db775fd8ff1d18ffb6de2b9ca0e                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7                                      │
│ Branches  cloudhead/draft                                                               │
│ Commits   ahead 1, behind 0                                                             │
│ Status    open                                                                          │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                                                           │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [   ...    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

If for whatever reason, it needed to go back into draft mode, we could use
the `--undo` flag:

```
$ rad patch ready --undo 6d7793b859860db775fd8ff1d18ffb6de2b9ca0e
$ rad patch show 6d7793b859860db775fd8ff1d18ffb6de2b9ca0e
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Nothing yet                                                                   │
│ Patch     6d7793b859860db775fd8ff1d18ffb6de2b9ca0e                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7                                      │
│ Branches  cloudhead/draft                                                               │
│ Commits   ahead 1, behind 0                                                             │
│ Status    draft                                                                         │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                                                           │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [   ...    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────╯

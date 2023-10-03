Let's say we have some changes in a branch:

```
$ git checkout -b cloudhead/draft
$ git commit -a -m "Nothing to see here.." -q --allow-empty
```

To open a patch in draft mode, we use the `--draft` option:

``` (stderr)
$ git push -o patch.draft -o patch.message="Nothing yet" rad HEAD:refs/patches
✓ Patch 78fcb007b4a3a898379f1e220d4b9fb54ad04cfc drafted
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can confirm it's a draft by running `show`:

```
$ rad patch show 78fcb007b4a3a898379f1e220d4b9fb54ad04cfc
╭────────────────────────────────────────────────────╮
│ Title     Nothing yet                              │
│ Patch     78fcb007b4a3a898379f1e220d4b9fb54ad04cfc │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7 │
│ Branches  cloudhead/draft                          │
│ Commits   ahead 1, behind 0                        │
│ Status    draft                                    │
├────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                      │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) [   ...    ]     │
╰────────────────────────────────────────────────────╯
```

Once the patch is ready for review, we can use the `ready` command:

```
$ rad patch ready 78fcb007b4a3a898379f1e220d4b9fb54ad04cfc
```

```
$ rad patch show 78fcb007b4a3a898379f1e220d4b9fb54ad04cfc
╭────────────────────────────────────────────────────╮
│ Title     Nothing yet                              │
│ Patch     78fcb007b4a3a898379f1e220d4b9fb54ad04cfc │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7 │
│ Branches  cloudhead/draft                          │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                      │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) [   ...    ]     │
╰────────────────────────────────────────────────────╯
```

If for whatever reason, it needed to go back into draft mode, we could use
the `--undo` flag:

```
$ rad patch ready --undo 78fcb007b4a3a898379f1e220d4b9fb54ad04cfc
$ rad patch show 78fcb007b4a3a898379f1e220d4b9fb54ad04cfc
╭────────────────────────────────────────────────────╮
│ Title     Nothing yet                              │
│ Patch     78fcb007b4a3a898379f1e220d4b9fb54ad04cfc │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7 │
│ Branches  cloudhead/draft                          │
│ Commits   ahead 1, behind 0                        │
│ Status    draft                                    │
├────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                      │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) [   ...    ]     │
╰────────────────────────────────────────────────────╯

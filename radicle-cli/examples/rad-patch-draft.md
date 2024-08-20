Let's say we have some changes in a branch:

```
$ git checkout -b cloudhead/draft
$ git commit -a -m "Nothing to see here.." -q --allow-empty
```

To open a patch in draft mode, we use the `--draft` option:

``` (stderr)
$ git push -o patch.draft -o patch.message="Nothing yet" rad HEAD:refs/patches
✓ Patch acee9948a4ff68e49a678734e8a0b86ff29f2e40 drafted
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can confirm it's a draft by running `show`:

```
$ rad patch show acee9948a4ff68e49a678734e8a0b86ff29f2e40
╭────────────────────────────────────────────────────╮
│ Title     Nothing yet                              │
│ Patch     acee9948a4ff68e49a678734e8a0b86ff29f2e40 │
│ Author    alice (you)                              │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7 │
│ Branches  cloudhead/draft                          │
│ Commits   ahead 1, behind 0                        │
│ Status    draft                                    │
├────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                      │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (2a46583) [ .. ]           │
╰────────────────────────────────────────────────────╯
```

Once the patch is ready for review, we can use the `ready` command:

```
$ rad patch ready acee9948a4ff68e49a678734e8a0b86ff29f2e40 --no-announce
```

```
$ rad patch show acee9948a4ff68e49a678734e8a0b86ff29f2e40
╭────────────────────────────────────────────────────╮
│ Title     Nothing yet                              │
│ Patch     acee9948a4ff68e49a678734e8a0b86ff29f2e40 │
│ Author    alice (you)                              │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7 │
│ Branches  cloudhead/draft                          │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                      │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (2a46583) [ .. ]           │
╰────────────────────────────────────────────────────╯
```

If for whatever reason, it needed to go back into draft mode, we could use
the `--undo` flag:

```
$ rad patch ready --undo acee9948a4ff68e49a678734e8a0b86ff29f2e40 --no-announce
$ rad patch show acee9948a4ff68e49a678734e8a0b86ff29f2e40
╭────────────────────────────────────────────────────╮
│ Title     Nothing yet                              │
│ Patch     acee9948a4ff68e49a678734e8a0b86ff29f2e40 │
│ Author    alice (you)                              │
│ Head      2a465832b5a76abe25be44a3a5d224bbd7741ba7 │
│ Branches  cloudhead/draft                          │
│ Commits   ahead 1, behind 0                        │
│ Status    draft                                    │
├────────────────────────────────────────────────────┤
│ 2a46583 Nothing to see here..                      │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (2a46583) [ .. ]           │
╰────────────────────────────────────────────────────╯
```

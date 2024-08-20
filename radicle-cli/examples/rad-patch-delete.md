``` ~alice
$ git checkout -b prepare-license
$ touch LICENSE
$ git add LICENSE
$ git commit -m "Introduce license"
[prepare-license 717c900] Introduce license
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

``` ~alice (stderr)
$ git push rad -o patch.draft -o patch.message="Define LICENSE for project" HEAD:refs/patches
✓ Patch e5dc5fd15fbe952da6a0f43934eae57d47b93e36 drafted
✓ Synced with 2 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` ~bob
$ cd heartwood
$ rad sync -f
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
$ rad patch comment e5dc5fd -m "I think we should use MIT"
╭───────────────────────────╮
│ bob (you) now 2ec2cc1     │
│ I think we should use MIT │
╰───────────────────────────╯
✓ Synced with 2 node(s)
```

``` ~alice
$ rad patch show e5dc5fd -v
╭────────────────────────────────────────────────────╮
│ Title     Define LICENSE for project               │
│ Patch     e5dc5fd15fbe952da6a0f43934eae57d47b93e36 │
│ Author    alice (you)                              │
│ Head      717c900ec17735639587325e0fd9fe09991c9edd │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ Branches  prepare-license                          │
│ Commits   ahead 1, behind 0                        │
│ Status    draft                                    │
├────────────────────────────────────────────────────┤
│ 717c900 Introduce license                          │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (717c900) now              │
├────────────────────────────────────────────────────┤
│ bob z6Mkt67…v4N1tRk now 2ec2cc1                    │
│ I think we should use MIT                          │
╰────────────────────────────────────────────────────╯
$ rad patch comment e5dc5fd --reply-to 2ec2cc1 -m "Thanks, I'll add it!"
╭─────────────────────────╮
│ alice (you) now 737dcab │
│ Thanks, I'll add it!    │
╰─────────────────────────╯
✓ Synced with 2 node(s)
```

``` ~alice
$ touch MIT
$ ln MIT LICENSE -f
$ git add MIT
$ git commit -am "Add MIT License"
[prepare-license 1cc8cd9] Add MIT License
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 MIT
```

``` ~alice (stderr)
$ git push -f
✓ Patch e5dc5fd updated to revision 1a1082a96f552767d352d69b8e6524aeb82f67a4
To compare against your previous revision e5dc5fd, run:

   git range-diff f2de534[..] 717c900[..] 1cc8cd9[..]

✓ Synced with 2 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   717c900..1cc8cd9  prepare-license -> patches/e5dc5fd15fbe952da6a0f43934eae57d47b93e36
```

``` ~bob
$ rad patch review e5dc5fd --accept -m "LGTM!"
✓ Patch e5dc5fd accepted
✓ Synced with 2 node(s)
$ rad patch show e5dc5fd -v
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Define LICENSE for project                                 │
│ Patch    e5dc5fd15fbe952da6a0f43934eae57d47b93e36                   │
│ Author   alice z6MknSL…StBU8Vi                                      │
│ Head     1cc8cd9de8ccc44b4fe3876f2dbd2cd1cf9ddc0e                   │
│ Base     f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354                   │
│ Commits  ahead 2, behind 0                                          │
│ Status   draft                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 1cc8cd9 Add MIT License                                             │
│ 717c900 Introduce license                                           │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice z6MknSL…StBU8Vi (717c900) now                     │
│ ↑ updated to 1a1082a96f552767d352d69b8e6524aeb82f67a4 (1cc8cd9) now │
│   └─ ✓ accepted by bob (you) now                                    │
╰─────────────────────────────────────────────────────────────────────╯
```

``` ~bob
$ rad patch delete e5dc5fd
✓ Synced with 2 node(s)
```

``` ~alice
$ rad patch show e5dc5fd -v
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define LICENSE for project                                │
│ Patch     e5dc5fd15fbe952da6a0f43934eae57d47b93e36                  │
│ Author    alice (you)                                               │
│ Head      1cc8cd9de8ccc44b4fe3876f2dbd2cd1cf9ddc0e                  │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354                  │
│ Branches  prepare-license                                           │
│ Commits   ahead 2, behind 0                                         │
│ Status    draft                                                     │
├─────────────────────────────────────────────────────────────────────┤
│ 1cc8cd9 Add MIT License                                             │
│ 717c900 Introduce license                                           │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (717c900) now                               │
│ ↑ updated to 1a1082a96f552767d352d69b8e6524aeb82f67a4 (1cc8cd9) now │
╰─────────────────────────────────────────────────────────────────────╯
```

If Alice also decides to delete the patch, then any seeds that have synced with
Alice should no longer have the patch:

``` ~alice
$ rad patch delete e5dc5fd
✓ Synced with 2 node(s)
```

``` ~seed (fails)
$ rad patch show --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 e5dc5fd -v
✗ Error: Patch `e5dc5fd15fbe952da6a0f43934eae57d47b93e36` not found
```

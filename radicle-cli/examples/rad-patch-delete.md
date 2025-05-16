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
✓ Patch 6c61ef1716ad8a5c11e04dd7a3fec51e01fba70b drafted
✓ Synced with 2 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` ~bob
$ cd heartwood
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 replica(s)
🌱 Fetched from z6MknSL…StBU8Vi
🌱 Fetched from z6Mkux1…nVhib7Z
$ rad patch comment 6c61ef1 -m "I think we should use MIT"
╭───────────────────────────╮
│ bob (you) now 833db19     │
│ I think we should use MIT │
╰───────────────────────────╯
✓ Synced with 2 node(s)
```

``` ~alice
$ rad patch show 6c61ef1 -v
╭────────────────────────────────────────────────────╮
│ Title     Define LICENSE for project               │
│ Patch     6c61ef1716ad8a5c11e04dd7a3fec51e01fba70b │
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
│ bob z6Mkt67…v4N1tRk now 833db19                    │
│ I think we should use MIT                          │
╰────────────────────────────────────────────────────╯
$ rad patch comment 6c61ef1 --reply-to 833db19 -m "Thanks, I'll add it!"
╭─────────────────────────╮
│ alice (you) now 1803a38 │
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
✓ Patch 6c61ef1 updated to revision 93915b9afa94a9dc4f52f12cdf077d4613ea3eb3
To compare against your previous revision 6c61ef1, run:

   git range-diff f2de534[..] 717c900[..] 1cc8cd9[..]

✓ Synced with 2 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   717c900..1cc8cd9  prepare-license -> patches/6c61ef1716ad8a5c11e04dd7a3fec51e01fba70b
```

``` ~bob
$ rad patch review 6c61ef1 --accept -m "LGTM!"
✓ Patch 6c61ef1 accepted
✓ Synced with 2 node(s)
$ rad patch show 6c61ef1 -v
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Define LICENSE for project                                 │
│ Patch    6c61ef1716ad8a5c11e04dd7a3fec51e01fba70b                   │
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
│ ↑ updated to 93915b9afa94a9dc4f52f12cdf077d4613ea3eb3 (1cc8cd9) now │
│   └─ ✓ accepted by bob (you) now                                    │
╰─────────────────────────────────────────────────────────────────────╯
```

``` ~bob
$ rad patch delete 6c61ef1
✓ Synced with 2 node(s)
```

``` ~alice
$ rad patch show 6c61ef1 -v
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define LICENSE for project                                │
│ Patch     6c61ef1716ad8a5c11e04dd7a3fec51e01fba70b                  │
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
│ ↑ updated to 93915b9afa94a9dc4f52f12cdf077d4613ea3eb3 (1cc8cd9) now │
╰─────────────────────────────────────────────────────────────────────╯
```

If Alice also decides to delete the patch, then any seeds that have synced with
Alice should no longer have the patch:

``` ~alice
$ rad patch delete 6c61ef1
✓ Synced with 2 node(s)
```

``` ~seed (fails)
$ rad patch show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji 6c61ef1 -v
✗ Error: Patch `6c61ef1716ad8a5c11e04dd7a3fec51e01fba70b` not found
```

``` ~alice
$ git checkout -b prepare-license
$ touch LICENSE
$ git add LICENSE
$ git commit -m "Introduce license"
[prepare-license 02edd08] Introduce license
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

``` ~alice (stderr)
$ git push rad -o patch.draft -o patch.message="Define LICENSE for project" HEAD:refs/patches
✓ Patch 4a503164d6e7e04122111af55fc20efb3fe23553 drafted
✓ Synced with 2 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` ~bob
$ cd heartwood
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
🌱 Fetched from z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
$ rad patch comment 4a50316 -m "I think we should use MIT"
╭───────────────────────────╮
│ bob (you) now 9f849ee     │
│ I think we should use MIT │
╰───────────────────────────╯
✓ Synced with 2 seed(s)
```

``` ~alice
$ rad patch show 4a50316 -v
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Define LICENSE for project                                                                                                                                 │
│ Patch     4a503164d6e7e04122111af55fc20efb3fe23553                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      02edd08b99de0a5220257196d49ff29dd08bd8f0                                                                                                                   │
│ Base      4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927                                                                                                                   │
│ Target    refs/heads/master                                                                                                                                          │
│ Branches  prepare-license                                                                                                                                            │
│ Commits   ahead 1, behind 0                                                                                                                                          │
│ Status    draft                                                                                                                                                      │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 02edd08 Introduce license                                                                                                                                            │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 4a503164d6e7e04122111af55fc20efb3fe23553 with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..02edd08b99de0a5220257196d49ff29dd08bd8f0 by alice (you) now │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ bob z6Mkt67…v4N1tRk now 9f849ee                                                                                                                                      │
│ I think we should use MIT                                                                                                                                            │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
$ rad patch comment 4a50316 --reply-to 9f849ee -m "Thanks, I'll add it!"
╭─────────────────────────╮
│ alice (you) now 96e3743 │
│ Thanks, I'll add it!    │
╰─────────────────────────╯
✓ Synced with 2 seed(s)
```

``` ~alice
$ touch MIT
$ git add MIT
$ git commit -am "Add MIT License"
[prepare-license e2ce7ab] Add MIT License
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 MIT
```

``` ~alice (stderr)
$ git push -f
✓ Patch 4a50316 updated to revision a913b2865234addc2a82eced5e39ca5740a5c6bd
To compare against your previous revision 4a50316, run:

   git range-diff 4c66f0e[..] 02edd08[..] e2ce7ab[..]

✓ Synced with 2 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   02edd08..e2ce7ab  prepare-license -> patches/4a503164d6e7e04122111af55fc20efb3fe23553
```

``` ~bob
$ rad patch review 4a50316 --accept -m "LGTM!"
✓ Patch 4a50316 accepted
✓ Synced with 2 seed(s)
$ rad patch show 4a50316 -v
╭─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title    Define LICENSE for project                                                                                                                                                                             │
│ Patch    4a503164d6e7e04122111af55fc20efb3fe23553                                                                                                                                                               │
│ Author   alice z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                                                                                                                                                 │
│ Head     e2ce7abf6e424a67102e07cfc3a504fd767e5d64                                                                                                                                                               │
│ Base     4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927                                                                                                                                                               │
│ Target   refs/heads/master                                                                                                                                                                                      │
│ Commits  ahead 2, behind 0                                                                                                                                                                                      │
│ Status   draft                                                                                                                                                                                                  │
├─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ e2ce7ab Add MIT License                                                                                                                                                                                         │
│ 02edd08 Introduce license                                                                                                                                                                                       │
├─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 4a503164d6e7e04122111af55fc20efb3fe23553 with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..02edd08b99de0a5220257196d49ff29dd08bd8f0 by alice z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi now │
│ ↑ Revision a913b2865234addc2a82eced5e39ca5740a5c6bd with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..e2ce7abf6e424a67102e07cfc3a504fd767e5d64 by alice z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi now │
│   └─ ✓ accepted by bob (you) now                                                                                                                                                                                │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

``` ~bob
$ rad patch delete 4a50316
✓ Synced with 2 seed(s)
```

``` ~alice
$ rad patch show 4a50316 -v
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Define LICENSE for project                                                                                                                                 │
│ Patch     4a503164d6e7e04122111af55fc20efb3fe23553                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      e2ce7abf6e424a67102e07cfc3a504fd767e5d64                                                                                                                   │
│ Base      4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927                                                                                                                   │
│ Target    refs/heads/master                                                                                                                                          │
│ Branches  prepare-license                                                                                                                                            │
│ Commits   ahead 2, behind 0                                                                                                                                          │
│ Status    draft                                                                                                                                                      │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ e2ce7ab Add MIT License                                                                                                                                              │
│ 02edd08 Introduce license                                                                                                                                            │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 4a503164d6e7e04122111af55fc20efb3fe23553 with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..02edd08b99de0a5220257196d49ff29dd08bd8f0 by alice (you) now │
│ ↑ Revision a913b2865234addc2a82eced5e39ca5740a5c6bd with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..e2ce7abf6e424a67102e07cfc3a504fd767e5d64 by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

If Alice also decides to delete the patch, then any seeds that have synced with
Alice should no longer have the patch:

``` ~alice
$ rad patch delete 4a50316
✓ Synced with 2 seed(s)
```

``` ~seed (fails)
$ rad patch show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji 4a50316 -v
✗ Error: Patch `4a503164d6e7e04122111af55fc20efb3fe23553` not found
```

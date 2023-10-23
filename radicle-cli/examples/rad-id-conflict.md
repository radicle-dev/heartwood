First let's add Bob as a delegate, and sync the changes to Bob:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --threshold 2 -q
bd41a1cc152a7cb8b9fb84261e4214ffa4cdb7a4
```
``` ~bob
$ cd heartwood
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
```

One thing that can happen is that two delegates propose a revision at the same
time:

``` ~alice
$ rad id update --title "Edit project name" --description "" --payload "xyz.radicle.project" "name" '"heart"' -q
6c07e4e604d855f6730f884dc56216c5698ef7f8
```
``` ~bob
$ rad id update --title "Edit project name" --description "" --payload "xyz.radicle.project" "name" '"wood"' -q
fae22d07f7d386b89f14ac353b079c9eef71f948
```

When Alice syncs with Bob, she notices the problem: there are two active
revisions.

``` ~alice
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
$ rad id list
╭─────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title               Author                     Status     Created │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ●   fae22d0   Edit project name   bob      z6Mkt67…v4N1tRk   active     now     │
│ ●   6c07e4e   Edit project name   alice    (you)             active     now     │
│ ●   bd41a1c   Add Bob             alice    (you)             accepted   now     │
│ ●   2317f74   Initial revision    alice    (you)             accepted   now     │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

This isn't a problem as long as we don't try to accept both. So let's accept
Bob's:

``` ~alice
$ rad id accept fae22d0 -q
$ rad id list
╭─────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title               Author                     Status     Created │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ●   fae22d0   Edit project name   bob      z6Mkt67…v4N1tRk   accepted   now     │
│ ●   6c07e4e   Edit project name   alice    (you)             stale      now     │
│ ●   bd41a1c   Add Bob             alice    (you)             accepted   now     │
│ ●   2317f74   Initial revision    alice    (you)             accepted   now     │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

Doing so voided the other conflicting revision, and it can no longer be
accepted now.

``` ~bob
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
```
``` ~bob (fail)
$ rad id accept 6c07e4e -q
✗ Error: cannot vote on revision that is stale
$ rad id reject 6c07e4e -q
✗ Error: cannot vote on revision that is stale
```
``` ~bob
$ rad id show 6c07e4e
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Edit project name                                             │
│ Revision 6c07e4e604d855f6730f884dc56216c5698ef7f8                      │
│ Blob     e93aa3e3c5c448bacd3537a81daf1437eccd046a                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    stale                                                         │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice       │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob   (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,14 +1,14 @@
 {
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
-      "name": "heartwood"
+      "name": "heart"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
     "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
   ],
   "threshold": 2
 }
```

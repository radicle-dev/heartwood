First let's add Bob as a delegate, and sync the changes to Bob:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
ba5c358894e0a58dd0772fd3eb6d070282dffc26
```
``` ~bob
$ cd heartwood
$ rad sync --fetch rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
```

One thing that can happen is that two delegates propose a revision at the same
time:

``` ~alice
$ rad id update --title "Edit project name" --description "" --payload "xyz.radicle.project" "name" '"heart"' -q
46b0a1a441cd1646395e3cf893b99aa258ed7c63
```
``` ~bob
$ rad id update --title "Edit project name" --description "" --payload "xyz.radicle.project" "name" '"wood"' -q
8118b11afaf5e43d4446788e0a223ed85e060a56
```

When Alice syncs with Bob, she notices the problem: there are two active
revisions.

``` ~alice
$ rad sync --fetch rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkt67…v4N1tRk@[..]..
✓ Fetched repository from 1 seed(s)
$ rad id list
╭─────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title               Author                     Status     Created │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ●   8118b11   Edit project name   bob      z6Mkt67…v4N1tRk   active     now     │
│ ●   46b0a1a   Edit project name   alice    (you)             active     now     │
│ ●   ba5c358   Add Bob             alice    (you)             accepted   now     │
│ ●   eeb8b44   Initial revision    alice    (you)             accepted   now     │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

This isn't a problem as long as we don't try to accept both. So let's accept
Bob's:

``` ~alice
$ rad id accept 8118b11 -q
$ rad id list
╭─────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title               Author                     Status     Created │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ●   8118b11   Edit project name   bob      z6Mkt67…v4N1tRk   accepted   now     │
│ ●   46b0a1a   Edit project name   alice    (you)             stale      now     │
│ ●   ba5c358   Add Bob             alice    (you)             accepted   now     │
│ ●   eeb8b44   Initial revision    alice    (you)             accepted   now     │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

Doing so voided the other conflicting revision, and it can no longer be
accepted now.

``` ~bob
$ rad sync --fetch rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
```
``` ~bob (fail)
$ rad id accept 46b0a1a -q
✗ Error: cannot vote on revision that is stale
$ rad id reject 46b0a1a -q
✗ Error: cannot vote on revision that is stale
```
``` ~bob
$ rad id show 46b0a1a
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Edit project name                                             │
│ Revision 46b0a1a441cd1646395e3cf893b99aa258ed7c63                      │
│ Blob     9ae843924509f3f617b044923772171287dab49b                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    stale                                                         │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice       │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob   (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,22 +1,22 @@
 {
   "version": 2,
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
   "canonicalRefs": {
     "rules": {
       "refs/heads/master": {
         "allow": "delegates",
         "threshold": 1
       }
     }
   }
 }
```

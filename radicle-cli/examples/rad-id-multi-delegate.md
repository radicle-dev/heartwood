``` ~alice
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Add Bob" --description "" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
f48a2c516aceccde576d9ba8845b21eca1f7902c
```

``` ~bob
$ rad watch --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --node z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z -r 'refs/rad/sigrefs' -t 6001fa5f08133dcf91029b4fc0b78a59bfd7883a -i 500 --timeout 5000
$ rad sync --fetch rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
$ rad id --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   f48a2c5   Add Bob            alice    z6MknSL…StBU8Vi   accepted   now     │
│ ●   eeb8b44   Initial revision   alice    z6MknSL…StBU8Vi   accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi [..]
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk [..]
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z [..]
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Add Eve" --description "" --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --no-confirm
✓ Identity revision 4e7e2aca58c18add67cf117ad414e61645cc39c0 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision 4e7e2aca58c18add67cf117ad414e61645cc39c0                      │
│ Blob     bad2d965c9022797a711cb2031c041ab2e2d729f                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob   (you) │
│ ? did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice       │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,22 +1,23 @@
 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
-    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
+    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
+    "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
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

``` ~alice
$ rad sync --fetch rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkux1…nVhib7Z@[..]..
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkt67…v4N1tRk@[..]..
✓ Fetched repository from 2 seed(s)
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad id accept 4e7e2aca58c18add67cf117ad414e61645cc39c0 --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --no-confirm
✓ Revision 4e7e2aca58c18add67cf117ad414e61645cc39c0 accepted
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision 4e7e2aca58c18add67cf117ad414e61645cc39c0                      │
│ Blob     bad2d965c9022797a711cb2031c041ab2e2d729f                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
╰────────────────────────────────────────────────────────────────────────╯
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z (eve)
```

``` ~alice
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Make private" --description "" --visibility private --no-confirm -q
61a7e9b58b27baf26b6f2e198aea3978e5d4444f
```

We can list all revisions:

``` ~alice
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   61a7e9b   Make private       alice    (you)             active     now     │
│ ●   4e7e2ac   Add Eve            bob      z6Mkt67…v4N1tRk   accepted   now     │
│ ●   f48a2c5   Add Bob            alice    (you)             accepted   now     │
│ ●   eeb8b44   Initial revision   alice    (you)             accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
```

Despite being a delegate, Bob can't edit or redact Alice's revision:

``` ~bob (fail)
$ rad id redact 61a7e9b58b27baf26b6f2e198aea3978e5d4444f
[..]
```
``` ~bob (fail)
$ rad id edit --title "Boo!" --description "Boo!" 61a7e9b58b27baf26b6f2e198aea3978e5d4444f
[..]
```

Alice can edit:

``` ~alice
$ rad id edit --title "Make private" --description "Privacy is cool." 61a7e9b58b27baf26b6f2e198aea3978e5d4444f
✓ Revision 61a7e9b58b27baf26b6f2e198aea3978e5d4444f edited
$ rad id show 61a7e9b58b27baf26b6f2e198aea3978e5d4444f
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Make private                                                  │
│ Revision 61a7e9b58b27baf26b6f2e198aea3978e5d4444f                      │
│ Blob     94448eb3f8b81fba6d25c287c626625fd3f53d8f                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
│                                                                        │
│ Privacy is cool.                                                       │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
│ ? did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z eve         │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,23 +1,26 @@
 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
     "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
     "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
   ],
   "canonicalRefs": {
     "rules": {
       "refs/heads/master": {
         "allow": "delegates",
         "threshold": 1
       }
     }
+  },
+  "visibility": {
+    "type": "private"
   }
 }
```

And she can redact her revision:

``` ~alice
$ rad id redact 61a7e9b58b27baf26b6f2e198aea3978e5d4444f
✓ Revision 61a7e9b58b27baf26b6f2e198aea3978e5d4444f redacted
```
``` ~alice (fail)
$ rad id show 61a7e9b58b27baf26b6f2e198aea3978e5d4444f
✗ Error: revision `61a7e9b58b27baf26b6f2e198aea3978e5d4444f` not found
```

Finally, Alice can also propose to remove Bob:
``` ~alice
$ rad id update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --title "Remove Bob" --description "" --rescind did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm
✓ Identity revision d8a5c75f44ee99bd66b9f7555066715c552b6fd8 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Remove Bob                                                    │
│ Revision d8a5c75f44ee99bd66b9f7555066715c552b6fd8                      │
│ Blob     14913e80b9585dfa545da9feb8ca68a42b5d085e                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
│ ? did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z eve         │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,23 +1,22 @@
 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
-    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
     "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
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

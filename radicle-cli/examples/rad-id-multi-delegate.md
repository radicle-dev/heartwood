``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
5666f744e2bc2333cab30ea0256bc4b61c3205bf
```

``` ~bob
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Eve" --description "" --delegate did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity revision e4b005ae70116fbe91340fef43127e86ab17e476 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision e4b005ae70116fbe91340fef43127e86ab17e476                      │
│ Blob     4c7fd4c7b7d7fd5d7088a7c952556fab99a034e9                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob   (you) │
│ ? did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice       │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,14 +1,15 @@
 {
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
+    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 2
 }
```

``` ~alice
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ rad id accept e4b005ae70116fbe91340fef43127e86ab17e476 --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-confirm
✓ Revision e4b005ae70116fbe91340fef43127e86ab17e476 accepted
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision e4b005ae70116fbe91340fef43127e86ab17e476                      │
│ Blob     4c7fd4c7b7d7fd5d7088a7c952556fab99a034e9                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
╰────────────────────────────────────────────────────────────────────────╯
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn
```

``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Make private" --description "" --visibility private --no-confirm -q
3fba3b17851cde9814f35acce4f740f53fb63dd7
```

We can list all revisions:

``` ~alice
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●   3fba3b1   Make private       alice    (you)             active     now     │
│ ●   e4b005a   Add Eve            bob      z6Mkt67…v4N1tRk   accepted   now     │
│ ●   5666f74   Add Bob            alice    (you)             accepted   now     │
│ ●   2317f74   Initial revision   alice    (you)             accepted   now     │
╰────────────────────────────────────────────────────────────────────────────────╯
```

Despite being a delegate, Bob can't edit or redact Alice's revision:

``` ~bob (fail)
$ rad id redact 3fba3b17851cde9814f35acce4f740f53fb63dd7
[..]
```
``` ~bob (fail)
$ rad id edit --title "Boo!" --description "Boo!" 3fba3b17851cde9814f35acce4f740f53fb63dd7
[..]
```

Alice can edit:

``` ~alice
$ rad id edit --title "Make private" --description "Privacy is cool." 3fba3b17851cde9814f35acce4f740f53fb63dd7
✓ Revision 3fba3b17851cde9814f35acce4f740f53fb63dd7 edited
$ rad id show 3fba3b17851cde9814f35acce4f740f53fb63dd7
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Make private                                                  │
│ Revision 3fba3b17851cde9814f35acce4f740f53fb63dd7                      │
│ Blob     79bc5c39103e811a3c9f11744f9a4029f063a5de                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
│                                                                        │
│ Privacy is cool.                                                       │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
│ ? did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn             │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,15 +1,18 @@
 {
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
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
-  "threshold": 2
+  "threshold": 2,
+  "visibility": {
+    "type": "private"
+  }
 }
```

And she can redact her revision:

``` ~alice
$ rad id redact 3fba3b17851cde9814f35acce4f740f53fb63dd7
✓ Revision 3fba3b17851cde9814f35acce4f740f53fb63dd7 redacted
```
``` ~alice (fail)
$ rad id show 3fba3b17851cde9814f35acce4f740f53fb63dd7
✗ Error: revision `3fba3b17851cde9814f35acce4f740f53fb63dd7` not found
```

Finally, Alice can also propose to remove Bob:
``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Remove Bob" --description "" --rescind did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm
✓ Identity revision 572b3189cf5a9cdba5f77db183a29a50562ba318 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Remove Bob                                                    │
│ Revision 572b3189cf5a9cdba5f77db183a29a50562ba318                      │
│ Blob     7109c1c201c223dd4e9fdb10f7330dc6f0310258                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
│ ? did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn             │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,15 +1,14 @@
 {
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
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 2
 }
```

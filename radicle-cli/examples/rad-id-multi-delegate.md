``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
5666f744e2bc2333cab30ea0256bc4b61c3205bf
```

``` ~bob
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add Eve" --description "" --delegate did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity revision 9d9031417f1d86a6c0ed5ec2c4bf5820dca0eec9 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision 9d9031417f1d86a6c0ed5ec2c4bf5820dca0eec9                      │
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
$ rad id accept 9d9031417f1d86a6c0ed5ec2c4bf5820dca0eec9 --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-confirm
✓ Revision 9d9031417f1d86a6c0ed5ec2c4bf5820dca0eec9 accepted
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Eve                                                       │
│ Revision 9d9031417f1d86a6c0ed5ec2c4bf5820dca0eec9                      │
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
1483814d54ad1321fc4ddb1f8bf7d90454de1790
```

We can list all revisions:

``` ~alice
$ rad id list
╭─────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title              Author                     Status     Created      │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ●   1483814   Make private       alice    (you)             active     [   ...    ] │
│ ●   9d90314   Add Eve            bob      z6Mkt67…v4N1tRk   accepted   [   ...    ] │
│ ●   5666f74   Add Bob            alice    (you)             accepted   [   ...    ] │
│ ●   2317f74   Initial revision   alice    (you)             accepted   [   ...    ] │
╰─────────────────────────────────────────────────────────────────────────────────────╯
```

Despite being a delegate, Bob can't edit or redact Alice's revision:

``` ~bob (fail)
$ rad id redact 1483814d54ad1321fc4ddb1f8bf7d90454de1790
[..]
```
``` ~bob (fail)
$ rad id edit --title "Boo!" --description "Boo!" 1483814d54ad1321fc4ddb1f8bf7d90454de1790
[..]
```

Alice can edit:

``` ~alice
$ rad id edit --title "Make private" --description "Privacy is cool." 1483814d54ad1321fc4ddb1f8bf7d90454de1790
✓ Revision 1483814d54ad1321fc4ddb1f8bf7d90454de1790 edited
$ rad id show 1483814d54ad1321fc4ddb1f8bf7d90454de1790
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Make private                                                  │
│ Revision 1483814d54ad1321fc4ddb1f8bf7d90454de1790                      │
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
$ rad id redact 1483814d54ad1321fc4ddb1f8bf7d90454de1790
✓ Revision 1483814d54ad1321fc4ddb1f8bf7d90454de1790 redacted
```
``` ~alice (fail)
$ rad id show 1483814d54ad1321fc4ddb1f8bf7d90454de1790
✗ Error: revision `1483814d54ad1321fc4ddb1f8bf7d90454de1790` not found
```

Finally, Alice can also propose to remove Bob:
``` ~alice
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Remove Bob" --description "" --rescind did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm
✓ Identity revision ea60049b8265f60f3dcca21798ce50ef67779421 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Remove Bob                                                    │
│ Revision ea60049b8265f60f3dcca21798ce50ef67779421                      │
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

Let's add a payload field and then delete it.

```
$ rad id update --title "Add field" --description "Add a new 'web' field" --payload xyz.radicle.project web '"https://acme.example"'
✓ Identity revision 4914cd968cd47b7f310946ba6c8e14269ca5a627 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add field                                                     │
│ Revision 4914cd968cd47b7f310946ba6c8e14269ca5a627                      │
│ Blob     74b79e158b1fd4ee3998dd541126d9e14a9ae976                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Add a new 'web' field                                                  │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,21 +1,22 @@
 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
-      "name": "heartwood"
+      "name": "heartwood",
+      "web": "https://acme.example"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
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

Now let's delete it by setting it to `null`.

```
$ rad id update --title "Delete field" --description "Delete 'web'" --payload xyz.radicle.project web null
✓ Identity revision 899c3a31664f6433bd92ccfd7fe02a3382de9e47 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Delete field                                                  │
│ Revision 899c3a31664f6433bd92ccfd7fe02a3382de9e47                      │
│ Blob     b38d81ee99d880461a3b7b3502e5d1556e440ef3                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Delete 'web'                                                           │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,22 +1,21 @@
 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
-      "name": "heartwood",
-      "web": "https://acme.example"
+      "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
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

Note that we cannot delete mandatory fields:

``` (fails)
$ rad id update --title "Delete default branch" --payload xyz.radicle.project defaultBranch null
✗ Error: failed to verify `xyz.radicle.project`, missing field `defaultBranch`
```

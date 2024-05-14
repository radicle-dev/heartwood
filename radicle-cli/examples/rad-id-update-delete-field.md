Let's add a payload field and then delete it.

```
$ rad id update --title "Add field" --description "Add a new 'web' field" --payload xyz.radicle.project web '"https://acme.example"'
✓ Identity revision a8a9fee6c4f83578ab132d375f1da0c81282bef3 created
╭───────────────────────────────────────────────────────────────────╮
│ Title    Add field                                                │
│ Revision a8a9fee6c4f83578ab132d375f1da0c81282bef3                 │
│ Blob     fbe268d13e60f1f3a1972e0ccd592f3cdecf08b5                 │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ State    accepted                                                 │
│ Quorum   yes                                                      │
│                                                                   │
│ Add a new 'web' field                                             │
├───────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi  (you) │
╰───────────────────────────────────────────────────────────────────╯

@@ -1,13 +1,14 @@
 {
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
   "threshold": 1
 }
```

Now let's delete it by setting it to `null`.

```
$ rad id update --title "Delete field" --description "Delete 'web'" --payload xyz.radicle.project web null
✓ Identity revision d373c35876833105f8aafed8b610660b5737cd67 created
╭───────────────────────────────────────────────────────────────────╮
│ Title    Delete field                                             │
│ Revision d373c35876833105f8aafed8b610660b5737cd67                 │
│ Blob     d96f425412c9f8ad5d9a9a05c9831d0728e2338d                 │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ State    accepted                                                 │
│ Quorum   yes                                                      │
│                                                                   │
│ Delete 'web'                                                      │
├───────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi  (you) │
╰───────────────────────────────────────────────────────────────────╯

@@ -1,14 +1,13 @@
 {
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
   "threshold": 1
 }
```

Note that we cannot delete mandatory fields:

``` (fails)
$ rad id update --title "Delete default branch" --payload xyz.radicle.project defaultBranch null
✗ Error: failed to verify `xyz.radicle.project`, failed with json: missing field `defaultBranch`
```

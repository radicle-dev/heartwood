The payloads in the identity document are extensible and arbitrary fields can be
added. Here we will add an emoji alias for the heartwood project:

```
$ rad id update --title "Add emoji alias" --description "Adding alias field" --payload xyz.radicle.project alias '"❤️🪵"'
✓ Identity revision d322c2b471685aa25d8874273b6ba2d70d5702de created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add emoji alias                                               │
│ Revision d322c2b471685aa25d8874273b6ba2d70d5702de                      │
│ Blob     92e06a7ec3b877ad66c77815ffd5270896d1d898                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Adding alias field                                                     │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,21 +1,22 @@
 {
   "version": 2,
   "payload": {
     "xyz.radicle.project": {
+      "alias": "❤️🪵",
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
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

We can see that the project payload still loads by using `rad ls` which will
attempt to deserialize the payload:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

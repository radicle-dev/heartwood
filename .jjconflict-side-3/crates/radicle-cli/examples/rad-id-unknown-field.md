The payloads in the identity document are extensible and arbitrary fields can be
added. Here we will add an emoji alias for the heartwood project:

```
$ rad id update --title "Add emoji alias" --description "Adding alias field" --payload xyz.radicle.project alias '"❤️🪵"'
✓ Identity revision 05100d3f0a73b9373681677158615a53ba51940e created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add emoji alias                                               │
│ Revision 05100d3f0a73b9373681677158615a53ba51940e                      │
│ Parent   0656c217f917c3e06234771e9ecae53aba5e173e                      │
│ Blob     a0f421c928dcfc6eca129fc2ea1f50877de7dc20                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Adding alias field                                                     │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,13 +1,14 @@
 {
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
   "threshold": 1
 }
```

We can see that the project payload still loads by using `rad ls` which will
attempt to deserialize the payload:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Let's say we have a private repo. To make it public, we use the `publish` command:

```
$ rad inspect --visibility
private
$ rad publish
✓ Repository is now public
! Warning: Your node is not running. Start your node with `rad node start` to announce your repository to the network
$ rad inspect --visibility
public
```

If we try to publish again, we get an error:

``` (fail)
$ rad publish
✗ Error: repository is already public
✗ Hint: to announce the repository to the network, run `rad sync --inventory`
```

We can also make the repository private again by using `rad id
update`. However, it's important to note that once the repository is
published to the network, that set of data will be public until all
node operators delete it. Any new changes made after making the
repository private again __will not_ be replicated.

```
$ rad id update --visibility private --title "Privatise" --description "Reverting the rad publish event"
✓ Identity revision 774cc1e72641d97d7dc9377745b7f454a9171747 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Privatise                                                     │
│ Revision 774cc1e72641d97d7dc9377745b7f454a9171747                      │
│ Blob     88f759a4d46e9535766fccec0cbfe1fed6160b1a                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Reverting the rad publish event                                        │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,13 +1,16 @@
 {
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "radicle heartwood protocol & stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
   ],
-  "threshold": 1
+  "threshold": 1,
+  "visibility": {
+    "type": "private"
+  }
 }
```

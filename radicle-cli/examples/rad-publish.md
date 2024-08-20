Let's say we have a private repo. To make it public, we use the `publish` command:

```
$ rad inspect --visibility
private
$ rad node inventory
$ rad publish
✓ Updating inventory..
✓ Repository is now public
! Warning: Your node is not running. Start your node with `rad node start` to announce your repository to the network
$ rad inspect --visibility
public
```

The repository is now in our inventory:
```
$ rad node inventory
rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT
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
✓ Identity revision 26ed629bb7c835bd94537e8226ca359c53b32cd3 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Privatise                                                     │
│ Revision 26ed629bb7c835bd94537e8226ca359c53b32cd3                      │
│ Blob     792ab13be76827e022b71941582a1b6217e6368a                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Reverting the rad publish event                                        │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,21 +1,24 @@
 {
   "version": 2,
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

``` ~alice
$ rad id update --title "Add canonical branch for releases" --payload xyz.radicle.crefs rules '{ "refs/heads/releases/*": { "threshold": 1, "allow": [ "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk" ] }, "refs/tags/*": { "threshold": 1, "allow": "delegates" }, "refs/tags/qa/*": { "threshold": 1, "allow": "delegates" }}'
✓ Identity revision [..] created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add canonical branch for releases                             │
│ Revision 37a1aad231100cd206c49aed79e405ea2da9204b                      │
│ Blob     bbefd77cfeb456e500ad868c3b4effe1f7f818e2                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,26 +1,32 @@
 {
   "payload": {
     "xyz.radicle.crefs": {
       "rules": {
+        "refs/heads/releases/*": {
+          "allow": [
+            "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
+          ],
+          "threshold": 1
+        },
         "refs/tags/*": {
           "allow": "delegates",
-          "threshold": 2
+          "threshold": 1
         },
         "refs/tags/qa/*": {
           "allow": "delegates",
           "threshold": 1
         }
       }
     },
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
     "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
     "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
   ],
   "threshold": 1
 }
```

``` ~bob
$ cd heartwood
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ rad id accept 37a1aad231100cd206c49aed79e405ea2da9204b -q
```

Bob immediately pushes a branch that matches the rule, thus populating
modifying the canonical namespace.

``` ~bob
$ git checkout -b releases/2
$ git commit --allow-empty --message "Release notes for version 2"
[releases/2 afec366] Release notes for version 2
```

``` ~bob (stderr)
$ git push -u rad releases/2
✓ Canonical reference refs/heads/releases/2 updated to target commit afec366785ed3651cdc66975c0fec41866c9ce62
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      releases/2 -> releases/2
```

``` ~alice 
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
afec366785ed3651cdc66975c0fec41866c9ce62	refs/heads/releases/2
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/tags/qa/v2.1
ac51a0746a5e8311829bc481202909a1e3acc0c2	refs/tags/v1.0-hotfix
89f935f27a16f8ed97915ade4accab8fe48057aa	refs/tags/v2.0
```
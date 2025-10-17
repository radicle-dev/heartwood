Bob overhears that the new name for the default branch is "main", not "master"
as it used to be. To make tooling that expect "main" work, without the hassle
of having to push to such branch manually, he opts to create a symbolic
reference.

``` ~bob
$ cd heartwood
$ rad id update --title "Add canonical symbolic ref" --payload xyz.radicle.crefs symbolic '{ "refs/heads/main": "refs/heads/master" }'
✓ Identity revision [..] created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add canonical symbolic ref                                    │
│ Revision 62e2cb60c6df9ad9908b6697b5d126760a855484                      │
│ Parent   37a1aad231100cd206c49aed79e405ea2da9204b                      │
│ Blob     b20de7b184673eb0d9227be17640c923d8ef3f3e                      │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk      │
│ State    active                                                        │
│ Quorum   no                                                            │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob   (you) │
│ ? did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice       │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,32 +1,35 @@
 {
   "payload": {
     "xyz.radicle.crefs": {
       "rules": {
         "refs/heads/releases/*": {
           "allow": [
             "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
           ],
           "threshold": 1
         },
         "refs/tags/*": {
           "allow": "delegates",
           "threshold": 1
         },
         "refs/tags/qa/*": {
           "allow": "delegates",
           "threshold": 1
         }
+      },
+      "symbolic": {
+        "refs/heads/main": "refs/heads/master"
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

Alice is happy with the new revision and accepts it.

``` ~alice
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ rad id accept 62e2cb60c6df9ad9908b6697b5d126760a855484 -q
```

As usual, alice works on "master".

``` ~alice
$ git commit --allow-empty --message "Whew, new feature!"
[master 4dc510d] Whew, new feature!
```

And updating the canonical reference for "master" also works as usual.

``` ~alice (stderr)
$ git push rad
✓ Canonical reference refs/heads/master updated to target commit 4dc510ddea5fd66499d1d2e996b8a97c8d57be54
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..4dc510d  master -> master
```

Then, Alice is curious about the new symbolic reference.
She inspects the remote and sees that indeed a new branch named "main" now exists.

``` ~alice 
$ git ls-remote rad
4dc510ddea5fd66499d1d2e996b8a97c8d57be54	HEAD
4dc510ddea5fd66499d1d2e996b8a97c8d57be54	refs/heads/main
4dc510ddea5fd66499d1d2e996b8a97c8d57be54	refs/heads/master
afec366785ed3651cdc66975c0fec41866c9ce62	refs/heads/releases/2
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/tags/qa/v2.1
ac51a0746a5e8311829bc481202909a1e3acc0c2	refs/tags/v1.0-hotfix
89f935f27a16f8ed97915ade4accab8fe48057aa	refs/tags/v2.0
```

Of course, she can also fetch it to her working copy as usual.

``` ~alice (stderr)
$ git fetch rad
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 * [new branch]      main       -> rad/main
 * [new branch]      releases/2 -> rad/releases/2
 * [new tag]         qa/v2.1    -> rad/tags/qa/v2.1
 * [new tag]         qa/v2.1    -> qa/v2.1
```

Bob fetches Alice's changes.

``` ~bob
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

And, sure enough, there is the new branch just as he wanted it.

``` ~bob (stderr)
$ git fetch rad
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 * [new branch]      main       -> rad/main
   f2de534..4dc510d  master     -> rad/master
```

Note that neither Alice nor Bob pushed directly to "main".

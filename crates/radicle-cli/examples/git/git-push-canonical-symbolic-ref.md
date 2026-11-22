Bob has a weird name for a particular release in mind, and also wants to test
out symbolic references.

``` ~bob
$ cd heartwood
$ rad id update --title "Add canonical symbolic ref" --payload xyz.radicle.crefs symbolic '{ "refs/heads/foobar": "refs/heads/releases/foobaz" }'
✓ Identity revision [..] created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add canonical symbolic ref                                    │
│ Revision b7597e3eeaba64467db4417602518169b76b43d5                      │
│ Parent   37a1aad231100cd206c49aed79e405ea2da9204b                      │
│ Blob     49a21614cf358b131fd6b590d0d01396f98906ce                      │
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
+        "refs/heads/foobar": "refs/heads/releases/foobaz"
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

Bob proceeds to push some commit to the branch that is targeted by the
symbolic reference. 

``` ~bob
$ git push -q rad master:releases/foobaz
```

Alice decides to play along with Bob's strange ideas and accepts the new
revision.

``` ~alice
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ rad id accept b7597e3eeaba64467db4417602518169b76b43d5 -q
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
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/foobar
4dc510ddea5fd66499d1d2e996b8a97c8d57be54	refs/heads/master
afec366785ed3651cdc66975c0fec41866c9ce62	refs/heads/releases/2
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/releases/foobaz
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/tags/qa/v2.1
ac51a0746a5e8311829bc481202909a1e3acc0c2	refs/tags/v1.0-hotfix
89f935f27a16f8ed97915ade4accab8fe48057aa	refs/tags/v2.0
```

Of course, she can also fetch it to her working copy as usual.

``` ~alice (stderr)
$ git fetch rad
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 * [new branch]      foobar          -> rad/foobar
 * [new branch]      releases/2      -> rad/releases/2
 * [new branch]      releases/foobaz -> rad/releases/foobaz
 * [new tag]         qa/v2.1         -> rad/tags/qa/v2.1
 * [new tag]         qa/v2.1         -> qa/v2.1
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
 * [new branch]      foobar     -> rad/foobar
   f2de534..4dc510d  master     -> rad/master
```

Note that neither Alice nor Bob pushed directly to "foobar".

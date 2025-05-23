In the attempt below, to update the identity, we can see that `a
threshold of 2 delegates cannot be met` when Alice attempts to do
this. This is because she requires Bob's default branch to ensure that
the threshold can be met and the canonical version of the default
branch (`refs/heads/<default branch>` at the top-level of the storage)
can be updated.

``` ~alice (fail)
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --threshold 2
✗ Error: failed to verify delegates for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✗ Error: the threshold of 2 delegates cannot be met..
✗ Error: the delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk is missing
✗ Hint: run `rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk` to follow this missing peer
✗ Error: fatal: refusing to update identity document
```

Instead, Alice can simply keep the `threshold` as `1` and still add Bob as a delegate:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Identity revision 7be665f9fccba97abb21b2fa85a6fd3181c72858 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Bob                                                       │
│ Revision 7be665f9fccba97abb21b2fa85a6fd3181c72858                      │
│ Blob     93d3009787e5d8a481dffc4dd248ea46af592466                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Add Bob as a delegate                                                  │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,13 +1,14 @@
 {
   "payload": {
     "xyz.radicle.project": {
       "defaultBranch": "master",
       "description": "Radicle Heartwood Protocol & Stack",
       "name": "heartwood"
     }
   },
   "delegates": [
-    "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
+    "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
+    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
   ],
   "threshold": 1
 }
```

Alice can still make changes to her working copy, change the canonical
head, and make patches -- as we can see below:

``` ~alice
$ touch REQUIREMENTS
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[master 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

``` ~alice (stderr) RAD_SOCKET=/dev/null
$ git push rad master
✓ Canonical head updated to 3e674d1a1df90807e934f9ae5da2591dd6848a33
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..3e674d1  master -> master
```

``` ~alice
$ rad sync -a
✓ Synced with 1 node(s)
```

``` ~alice
$ git checkout -b add-readme
$ touch README.md
$ git add README.md
$ git commit -v -m "Add README file"
[add-readme 964513c] Add README file
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` ~alice (stderr) RAD_SOCKET=/dev/null
$ git push rad HEAD:refs/patches
✓ Patch b09b2aa0ee055671c811e9ad4ba73eed211ebaa3 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Any other seeds can also still fetch changes from Alice without any
errors:

``` ~seed
$ rad sync rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSL…StBU8Vi
```

We can also inspect the repository to ensure all the data is
consistent with the change made to the identity.

The identity has the new delegate, while the `threshold` is still `1`

``` ~alice
$ rad inspect --identity
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
    "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
  ],
  "threshold": 1
}
```

Alice only sees her refs, since she has not synced with Bob's
references yet:

``` ~alice
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── 0656c217f917c3e06234771e9ecae53aba5e173e
    │   └── xyz.radicle.patch
    │       └── b09b2aa0ee055671c811e9ad4ba73eed211ebaa3
    ├── heads
    │   ├── master
    │   └── patches
    │       └── b09b2aa0ee055671c811e9ad4ba73eed211ebaa3
    └── rad
        ├── id
        ├── root
        └── sigrefs
```

Similarly, she still does not have Bob's `rad/sigrefs`:

``` ~alice
$ rad inspect --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi ae3c6b77dc1ed51c1c1e6a2772339c2779fa9ba8
```

And she can still list the project, without any worries:

``` ~alice
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   public       3e674d1   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Once Bob clones the repository and creates a fork, i.e. creates a
branch to `refs/heads/master` for this project, she can then use `rad
sync` and fetch his references:

``` ~bob
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 seed(s)
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [..]/bob/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 1 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
```

``` ~bob
$ cd heartwood
$ rad fork
✓ Forked repository rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

``` ~alice
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 seed(s)
🌱 Fetched from z6Mkux1…nVhib7Z
🌱 Fetched from z6Mkt67…v4N1tRk
$ rad inspect --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi ae3c6b77dc1ed51c1c1e6a2772339c2779fa9ba8
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk dace6fe948548168a2bb687718949d9b5d9076ee
```

In this example, we will show how we can make other references become canonical.
To illustrate, we will use annotated Git tags as an example. The storage of the repository
should look something like this by the end of the example:

~~~
storage/z6cFWeWpnZNHh9rUW8phgA3b5yGt/refs
├── heads
│   └── main
├── namespaces
│   ├── z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
│   │   └── refs
│   │       ├── cobs
│   │       │   └── xyz.radicle.id
│   │       │       └── 865c48204bd7bb7f088b8db90ffdccb48cfa0a50
│   │       ├── heads
│   │       │   └── master
│   │       ├── tags
│   │       │   ├── v1.0-hotfix
│   │       │   └── v1.0
│   │       └── rad
│   │           ├── id
│   │           └── sigrefs
│   └── z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
│       └── refs
│           ├── heads
│           │   └── master
│           ├── tags
│           │   ├── v1.0-hotfix
│           │   └── v1.0
│           └── rad
│               ├── id
│               └── sigrefs
├── rad
│   └── id
└── tags
    ├── v1.0-hotfix
    └── v1.0
~~~

Noting that there are tags under `refs/tags` now.

To start, Alice will add a new payload to the repository identity. The
identifier for this payload is `xyz.radicle.crefs`. It contains a single field
with the key `rules`, and the value for this key is an array of rules. In this
case, we will have two rules: one for `refs/tags/*` and one for `refs/tags/qa/*`
(see RIP-0004 for more information on the rules).

``` ~alice
$ rad id update --title "Add canonical reference rules" --payload xyz.radicle.crefs rules '{ "refs/tags/*": { "threshold": 1, "allow": "delegates" }, "refs/tags/qa/*": { "threshold": 1, "allow": "delegates" }}'
✓ Identity revision [..] created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add canonical reference rules                                 │
│ Revision c3349f07bfe6a82bbeb2989d2de4a918408f9831                      │
│ Blob     85fa09e2de93b825d5231778dbb34143004a4bca                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,13 +1,25 @@
 {
   "payload": {
+    "xyz.radicle.crefs": {
+      "rules": {
+        "refs/tags/*": {
+          "allow": "delegates",
+          "threshold": 1
+        },
+        "refs/tags/qa/*": {
+          "allow": "delegates",
+          "threshold": 1
+        }
+      }
+    },
     "xyz.radicle.project": {
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

Now, Alice will create an annotated tag and push it:

``` ~alice
$ git tag -a -m "Hotfix for release 1" v1.0-hotfix
$ git cat-file -t v1.0-hotfix
tag
```

``` ~alice (stderr)
$ git push rad --tags
✓ Canonical reference refs/tags/v1.0-hotfix updated to target tag ac51a0746a5e8311829bc481202909a1e3acc0c2
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new tag]         v1.0-hotfix -> v1.0-hotfix
```

Notice that the output included a message about a canonical reference being
updated:

~~~
✓ Canonical reference refs/tags/v1.0-hotfix updated to target tag ac51a0746a5e8311829bc481202909a1e3acc0c2
~~~

On the other side, Bob performs a fetch and now has the tags locally:

``` ~bob (stderr)
$ cd heartwood
$ git fetch rad
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 * [new tag]         v1.0-hotfix -> rad/tags/v1.0-hotfix
 * [new tag]         v1.0-hotfix -> v1.0-hotfix
```

Since Alice created an annotated tag, resolving the reference on Bob's end yields an object of type 'tag'.

``` ~bob
$ git cat-file -t v1.0-hotfix
tag
```

In the next portion of this example, we want to show that using a `threshold` of
`2` requires both delegates. To do this, Bob creates a `master` reference, Alice
adds him as a remote, and adds him to the identity delegates, as well as setting
the `threshold` to `2` for the `refs/tags/*` rule:

``` ~bob
$ rad remote add z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --name alice
✓ Follow policy updated for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Remote alice added
✓ Remote-tracking branch alice/master created for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ git push rad master
```

``` ~alice
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ rad id update --title "Add Bob" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
27ab0d77a95581c59ca9d30e679ceb06a9f758db
$ rad id update --title "Update canonical reference rules" --payload xyz.radicle.crefs rules '{ "refs/tags/*": { "threshold": 2, "allow": "delegates" }, "refs/tags/qa/*": { "threshold": 1, "allow": "delegates" } }' -q
dace164ba43fa51802697ec28d0b1965a9d7808b
```

**Note:** here we have to specify all the rules again to update the `threshold`.
In reality, you can use `rad id update --edit` and edit the payload in your
editor instead.

``` ~bob
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ rad id accept dace164ba43fa51802697ec28d0b1965a9d7808b -q
```

When Bob creates a new tag and pushes it, we see that there's a warning that
no quorum was found for the new tag:

``` ~bob
$ git tag -l
v1.0-hotfix
$ git rev-parse v1.0-hotfix
ac51a0746a5e8311829bc481202909a1e3acc0c2
```

``` ~bob (stderr)
$ git tag -a -m "Release 2" v2.0
$ git push rad --tags
warn: could not determine target for canonical reference 'refs/tags/v2.0', no object with at least 2 vote(s) found (threshold not met)
warn: it is recommended to find an object (either commit or tag) to agree upon
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new tag]         v1.0-hotfix -> v1.0-hotfix
 * [new tag]         v2.0 -> v2.0
```

Alice can then fetch and checkout the new tag, create one on her side, and push
it:

``` ~alice (stderr)
$ git fetch bob
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new tag]         v1.0-hotfix -> bob/tags/v1.0-hotfix
 * [new tag]         v2.0        -> bob/tags/v2.0
```

At this point Alice might check out `v2.0` and consider whether she agrees with Bob.
Let's say that Alice agrees, so she copies the tag to her repository using `git tag`:

``` ~alice
$ git tag v2.0 bob/tags/v2.0
```

``` ~alice (stderr)
$ git push rad --tags
✓ Canonical reference refs/tags/v2.0 updated to target tag 89f935f27a16f8ed97915ade4accab8fe48057aa
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new tag]         v2.0 -> v2.0
```

Now that Bob has also pushed this tag, we can see that the tag was made
canonical.

For the final portion of the example, we will show that both delegates aren't
required for pushing tags that match the rule `refs/tags/qa/*`. To show this,
Bob will create a lightweight tag and push it, and we should see that the canonical
reference is created:

``` ~bob (stderr)
$ git tag qa/v2.1
$ git push rad --tags
✓ Canonical reference refs/tags/qa/v2.1 updated to target commit f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new tag]         qa/v2.1 -> qa/v2.1
```

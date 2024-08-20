At some point in the lifetime of a Radicle project you may want to
collaborate with someone else allowing them to become a project
maintainer. This requires adding them as a `delegate`.

For changes made to the identity, a majority of delegate signatures is required.
For now, since we are the only delegate, we can update the identity ourselves.

Let's add Bob as a delegate using their DID,
`did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk`, and update the
threshold to `2`.

```
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Identity revision ba5c358894e0a58dd0772fd3eb6d070282dffc26 created
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Bob                                                       │
│ Revision ba5c358894e0a58dd0772fd3eb6d070282dffc26                      │
│ Blob     8aa049fbaa433f84073983964a54ab909cb2fe9a                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Add Bob as a delegate                                                  │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
│ ? did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk bob         │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,21 +1,22 @@
 {
   "version": 2,
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

Before moving on, let's take a few notes on this output. The first
thing we'll notice is that the difference between the current identity
document and the proposed changes are shown. Specifically, we changed
the delegates and threshold:

      "delegates": [
    -   "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
    +   "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
    +   "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ],
    ...
    -  "threshold": 1
    +  "threshold": 2

Next we have the number of signatures from delegates, which includes our own:

    ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Finally, we can see whether the `Quorum` was reached:

    Quorum   yes

Since the threshold was previously `1`, this change is now in effect. We
can verify that by listing the current identity document:

```
$ rad inspect --identity
{
  "version": 2,
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

We can also look at the document's COB directly:
```
$ rad cob log --object eeb8b44 --type xyz.radicle.id --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
commit   ba5c358894e0a58dd0772fd3eb6d070282dffc26
parent   eeb8b44890570ccf85db7f3cb2a475100a27408a
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "blob": "8aa049fbaa433f84073983964a54ab909cb2fe9a",
      "description": "Add Bob as a delegate",
      "parent": "eeb8b44890570ccf85db7f3cb2a475100a27408a",
      "signature": "z23hpnKuBai93fnjm6qJeTtPrT7hDeLUJQLmmoE8xbgFrKCUYjYf6ZrgFKZLL8PqhMnNJTJcfmrZcABUzum2SGiju",
      "title": "Add Bob",
      "type": "revision"
    }

commit   eeb8b44890570ccf85db7f3cb2a475100a27408a
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "blob": "b38d81ee99d880461a3b7b3502e5d1556e440ef3",
      "parent": null,
      "signature": "z246mVBUXJmr3YYeiTE7yuYteiHvA3bnqUWASB6VBnEbn6JB6eAxLv8mCGvCqaRL4BgVcn1Aho5fnVUqSdhR44SHv",
      "title": "Initial revision",
      "type": "revision"
    }

```

We can use `rad id show` to show the changes of an accepted update:

```
$ rad id show ba5c358894e0a58dd0772fd3eb6d070282dffc26
╭────────────────────────────────────────────────────────────────────────╮
│ Title    Add Bob                                                       │
│ Revision ba5c358894e0a58dd0772fd3eb6d070282dffc26                      │
│ Blob     8aa049fbaa433f84073983964a54ab909cb2fe9a                      │
│ Author   did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi      │
│ State    accepted                                                      │
│ Quorum   yes                                                           │
│                                                                        │
│ Add Bob as a delegate                                                  │
├────────────────────────────────────────────────────────────────────────┤
│ ✓ did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi alice (you) │
╰────────────────────────────────────────────────────────────────────────╯

@@ -1,21 +1,22 @@
 {
   "version": 2,
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

Note that once a revision is accepted, it can't be edited, redacted or otherwise
acted upon:

``` (fail)
$ rad id redact ba5c358894e0a58dd0772fd3eb6d070282dffc26
✗ Error: [..]
```
``` (fail)
$ rad id reject ba5c358894e0a58dd0772fd3eb6d070282dffc26
✗ Error: [..]
```
``` (fail)
$ rad id accept ba5c358894e0a58dd0772fd3eb6d070282dffc26
✗ Error: [..]
```

If no updates are specified then we are told that our command had no effect:

```
$ rad id update --title "Update canonical branch" --description "Update the canonical branch to `main`"
Nothing to do. The document is up to date. See `rad inspect --identity`.
```

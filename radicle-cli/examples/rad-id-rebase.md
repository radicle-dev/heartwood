In this example, we're going to see what happens when a proposal
drifts away from the latest Radicle identity.

First off, we will create two proposals -- we can imagine two
delegates creating proposals concurrently.

```
$ rad id edit --title "Add Alice" --description "Add Alice as a delegate" --delegates did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity proposal '04603c0d3ea4d137487024a51c9360adfc511114' created
title: Add Alice
description: Add Alice as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 0
keys: []

Rejected

total: 0
keys: []

Quorum Reached

👎 no
```

```
$ rad id edit --title "Add Bob" --description "Add Bob as a delegate" --delegates did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG --no-confirm
✓ Identity proposal '3f6ae4f8645c8b0cbcd35ea924df7b13aca52774' created
title: Add Bob
description: Add Bob as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"
   ],
   "threshold": 1
 }


Accepted

total: 0
keys: []

Rejected

total: 0
keys: []

Quorum Reached

👎 no
```

Now, if the first proposal was accepted and committed before the
second proposal, then the identity would be out of date. So let's run
through that and see what happens.

```
$ rad id accept 04603c0d3ea4d137487024a51c9360adfc511114 --no-confirm
✓ Accepted proposal ✓
title: Add Alice
description: Add Alice as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 1
keys: [
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

👍 yes
```

```
$ rad id commit 04603c0d3ea4d137487024a51c9360adfc511114 --no-confirm
✓ Committed new identity '29ae4b72f5a315328f06fbd68dc1c396a2d5c45e'
title: Add Alice
description: Add Alice as a delegate
status: ❲committed❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 1
keys: [
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

👍 yes
```

Now, when we go to accept the second proposal:

```
$ rad id accept 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --no-confirm
! Warning: Revision is out of date
! Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
👉 Consider using 'rad id rebase' to update the proposal to the latest identity
✓ Accepted proposal ✓
title: Add Bob
description: Add Bob as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
-    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
+    "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"
   ],
   "threshold": 1
 }


Accepted

total: 1
keys: [
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

👍 yes
```

Note that a warning was emitted:

    ** Warning: Revision is out of date
    ** Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
    => Consider using 'rad id rebase' to update the proposal to the latest identity

If we attempt to commit this revision, the command will fail:

```
$ rad id commit 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --no-confirm
! Warning: Revision is out of date
! Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
👉 Consider using 'rad id rebase' to update the proposal to the latest identity
✗ Id failed: the identity hashes do match 'd96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f' for the revision '3f6ae4f8645c8b0cbcd35ea924df7b13aca52774'
```

So, let's fix this by running a rebase on the proposal's revision:

```
$ rad id rebase 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --no-confirm
✓ Identity proposal '3f6ae4f8645c8b0cbcd35ea924df7b13aca52774' rebased
✓ Revision 'a6db848f8dba4ef2a9c16301e7d21ad87b707f1e'
title: Add Bob
description: Add Bob as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
-    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
+    "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"
   ],
   "threshold": 1
 }


Accepted

total: 0
keys: []

Rejected

total: 0
keys: []

Quorum Reached

👎 no
```

We can now update the proposal to have both keys in the delegates set:

```
$ rad id update 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --rev a6db848f8dba4ef2a9c16301e7d21ad87b707f1e --delegates did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity proposal '3f6ae4f8645c8b0cbcd35ea924df7b13aca52774' updated
✓ Revision '4bac5e3ef20e412e9421886fbb76dc7a64d6b5dc'
title: Add Bob
description: Add Bob as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 0
keys: []

Rejected

total: 0
keys: []

Quorum Reached

👎 no
```

Finally, we can accept and commit this proposal, creating the final
state of our new Radicle identity:

$ rad id show 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --revisions

```
$ rad id accept 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --rev 4bac5e3ef20e412e9421886fbb76dc7a64d6b5dc --no-confirm
✓ Accepted proposal ✓
title: Add Bob
description: Add Bob as a delegate
status: ❲open❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 1
keys: [
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

👍 yes
```

```
$ rad id commit 3f6ae4f8645c8b0cbcd35ea924df7b13aca52774 --rev 4bac5e3ef20e412e9421886fbb76dc7a64d6b5dc --no-confirm
✓ Committed new identity '60de897bc24898f6908fd1272633c0b15aa4096f'
title: Add Bob
description: Add Bob as a delegate
status: ❲committed❳
author: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

Document Diff

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
+    "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 1
keys: [
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

👍 yes
```

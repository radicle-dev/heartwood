In this example, we're going to see what happens when a proposal
drifts away from the latest Radicle identity.

First off, we will create two proposals -- we can imagine two
delegates creating proposals concurrently.

```
$ rad id edit --title "Add Alice" --description "Add Alice as a delegate" --delegates did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity proposal '6e4dfd0edbb0974f5a4fb0990133e0911b0992c0' created 🌱
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
✓ Identity proposal '6a687267b5063c036fdde90b75cc5eb153fa25e8' created 🌱
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
$ rad id accept 6e4dfd0edbb0974f5a4fb0990133e0911b0992c0 --no-confirm
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
$ rad id commit 6e4dfd0edbb0974f5a4fb0990133e0911b0992c0 --no-confirm
✓ Committed new identity '29ae4b72f5a315328f06fbd68dc1c396a2d5c45e' 🌱
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
$ rad id accept 6a687267b5063c036fdde90b75cc5eb153fa25e8 --no-confirm
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
$ rad id commit 6a687267b5063c036fdde90b75cc5eb153fa25e8 --no-confirm
! Warning: Revision is out of date
! Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
👉 Consider using 'rad id rebase' to update the proposal to the latest identity
✗ Id failed: the identity hashes do match 'd96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f' for the revision '6877dc63e001e7f7fcb285f5f530948b3d96b488'
```

So, let's fix this by running a rebase on the proposal's revision:

```
$ rad id rebase 6a687267b5063c036fdde90b75cc5eb153fa25e8 --no-confirm
✓ Identity proposal '6a687267b5063c036fdde90b75cc5eb153fa25e8' rebased 🌱
✓ Revision 'aaa890c3531f880c9901b162ab38016ceb559c9f'
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
$ rad id update 6a687267b5063c036fdde90b75cc5eb153fa25e8 --rev aaa890c3531f880c9901b162ab38016ceb559c9f --delegates did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity proposal '6a687267b5063c036fdde90b75cc5eb153fa25e8' updated 🌱
✓ Revision '24ad4a6ce84b1ce4b8cc754494c23f1079020a14'
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

$ rad id show 6a687267b5063c036fdde90b75cc5eb153fa25e8 --revisions

```
$ rad id accept 6a687267b5063c036fdde90b75cc5eb153fa25e8 --rev 24ad4a6ce84b1ce4b8cc754494c23f1079020a14 --no-confirm
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
$ rad id commit 6a687267b5063c036fdde90b75cc5eb153fa25e8 --rev 24ad4a6ce84b1ce4b8cc754494c23f1079020a14 --no-confirm
✓ Committed new identity '60de897bc24898f6908fd1272633c0b15aa4096f' 🌱
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

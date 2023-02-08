In this example, we're going to see what happens when a proposal
drifts away from the latest Radicle identity.

First off, we will create two proposals -- we can imagine two
delegates creating proposals concurrently.

```
$ rad id edit --title "Add Alice" --description "Add Alice as a delegate" --delegates z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
ok Identity proposal '57332790a2eabc0b2fd8c7ff48c3579d5812d405' created 🌱
title: Add Alice
description: Add Alice as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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

✗ no
```

```
$ rad id edit --title "Add Bob" --description "Add Bob as a delegate" --delegates z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG --no-confirm
ok Identity proposal 'c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e' created 🌱
title: Add Bob
description: Add Bob as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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

✗ no
```

Now, if the first proposal was accepted and committed before the
second proposal, then the identity would be out of date. So let's run
through that and see what happens.

```
$ rad id accept 57332790a2eabc0b2fd8c7ff48c3579d5812d405 --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
ok Accepted proposal ✓
title: Add Alice
description: Add Alice as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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
  "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

✓ yes
```

```
$ rad id commit 57332790a2eabc0b2fd8c7ff48c3579d5812d405 --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
ok Committed new identity 🌱
title: Add Alice
description: Add Alice as a delegate
status:  committed 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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
  "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

✓ yes
```

Now, when we go to accept the second proposal:

```
$ rad id accept c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
** Warning: Revision is out of date
** Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
=> Consider using 'rad id rebase' to update the proposal to the latest identity
ok Accepted proposal ✓
title: Add Bob
description: Add Bob as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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
  "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

✓ yes
```

Note that a warning was emitted:

    ** Warning: Revision is out of date
    ** Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
    => Consider using 'rad id rebase' to update the proposal to the latest identity

If we attempt to commit this revision, the command will fail:

```
$ rad id commit c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
** Warning: Revision is out of date
** Warning: d96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f
=> Consider using 'rad id rebase' to update the proposal to the latest identity
== Id failed
the identity hashes do match 'd96f425412c9f8ad5d9a9a05c9831d0728e2338d =/= 475cdfbc8662853dd132ec564e4f5eb0f152dd7f' for the revision 'z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1'
```

So, let's fix this by running a rebase on the proposal's revision:

```
$ rad id rebase c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
ok Identity proposal 'c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e' rebased 🌱
ok Revision 'z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/4'
title: Add Bob
description: Add Bob as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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

✗ no
```

We can now update the proposal to have both keys in the delegates set:

```
$ rad id update c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/4 --delegates z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
ok Identity proposal 'c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e' updated 🌱
ok Revision 'z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/6'
title: Add Bob
description: Add Bob as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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

✗ no
```

Finally, we can accept and commit this proposal, creating the final
state of our new Radicle identity:

$ rad id show c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --revisions

```
$ rad id accept c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/6 --no-confirm
ok Accepted proposal ✓
title: Add Bob
description: Add Bob as a delegate
status:  open 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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
  "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

✓ yes
```

```
$ rad id commit c3698d4e85f9d4c0ee536b34d6122fc7c81f7e2e --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/6 --no-confirm
ok Committed new identity 🌱
title: Add Bob
description: Add Bob as a delegate
status:  committed 
author: z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi

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
  "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Rejected

total: 0
keys: []

Quorum Reached

✓ yes
```

At some point in the lifetime of a Radicle project you may want to
collaborate with someone else allowing them to become a project
maintainer. This requires adding them as a `delegate` and possibly
editing the `threshold` for passing new changes to the identity of the
project.

For cases where `threshold = 1`, it is enough to use the `rad
delegate` command. For cases where `threshold > 1`, it is necessary to
gather a quorum of signatures to update the Radicle identity. To do
this, we use the `rad id` command.

Let's add Bob as a delegate using their key
`z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn`.

```
$ rad id edit --title "Add Bob" --description "Add Bob as a delegate" --delegates z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
ok Identity proposal '06d9efa2a9aad06bfdf25a25690e1ec7db2c3c39' created 🌱
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

Before moving on, let's take a few notes on this output. The first
thing we'll notice is that the difference between the current identity
document and the proposed changes are shown. Specifically, we changed
the delegates:

    "delegates": [
    -    "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
    +    "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
    +    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
    ],

Next we have the number of `Accepted` reviews from delegates, starting
off with none:

    Accepted
    
    total: 0
    keys: []

The same with `Rejected` reviews:

    Rejected
    
    total: 0
    keys: []

Finally, we can see whether the `Quorum` was reached:

    Quorum Reached
    
    ✗ no

Let's see what happens when we reject the change:

```
$ rad id reject 06d9efa2a9aad06bfdf25a25690e1ec7db2c3c39 --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
ok Rejected proposal ✗
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
+    "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
   "threshold": 1
 }


Accepted

total: 0
keys: []

Rejected

total: 1
keys: [
  "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Quorum Reached

✗ no
```

Our key was added to the `Rejected` set of `keys` and the `total`
increased to `1`.

    Rejected
    
    total: 1
    keys: [
      "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
    ]

Instead, let's accept the proposal:

```
$ rad id accept 06d9efa2a9aad06bfdf25a25690e1ec7db2c3c39 --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
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

Our key has changed from the `Rejected` set to the `Accepted` set
instead:

    Accepted
    
    total: 1
    keys: [
      "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
    ]

As well as that, the `Quorum` has now been reached:

    Quorum Reached
    
    ✓ yes

At this point, we can commit the proposal and update the identity:

```
$ rad id commit 06d9efa2a9aad06bfdf25a25690e1ec7db2c3c39 --rev z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/1 --no-confirm
ok Committed new identity 'c96e764965aaeff1c6ea3e5b97e2b9828773c8b0' 🌱
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

Let's say we decide to also change the `threshold`, we can do so using
the `--threshold` option:

```
$ rad id edit --title "Update threshold" --description "Update to safer threshold" --threshold 2 --no-confirm
ok Identity proposal 'dc00640d3152ea5f1df59f39f2f5983d2ad21810' created 🌱
title: Update threshold
description: Update to safer threshold
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
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
-  "threshold": 1
+  "threshold": 2
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

But we change our minds and decide to close the proposal instead:

```
$ rad id close dc00640d3152ea5f1df59f39f2f5983d2ad21810 --no-confirm
ok Closed identity proposal 'dc00640d3152ea5f1df59f39f2f5983d2ad21810'
title: Update threshold
description: Update to safer threshold
status:  closed 
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
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
-  "threshold": 1
+  "threshold": 2
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

The proposal is now closed and cannot be committed. If at a later date
we want to update the document with the same change we have to open a
new proposal.

If at any time we want to see what proposals have been made to this
Radicle identity, then we can use the list command:

```
$ rad id list
06d9efa2a9aad06bfdf25a25690e1ec7db2c3c39 "Add Bob"           committed
dc00640d3152ea5f1df59f39f2f5983d2ad21810 "Update threshold"  closed
```

And if we want to view the latest state of any proposal we can use the
show command:

```
$ rad id show dc00640d3152ea5f1df59f39f2f5983d2ad21810
title: Update threshold
description: Update to safer threshold
status:  closed 
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
     "did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn"
   ],
-  "threshold": 1
+  "threshold": 2
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

On a final note, these examples used `--no-confirm`. The default mode
for making proposals is to select and confirm any actions being
performed on the proposal.

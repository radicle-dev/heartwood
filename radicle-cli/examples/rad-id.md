At some point in the lifetime of a Radicle project you may want to
collaborate with someone else allowing them to become a project
maintainer. This requires adding them as a `delegate` and possibly
editing the `threshold` for passing new changes to the identity of the
project.

For cases where `threshold = 1`, it is enough to use the `rad
delegate` command. For cases where `threshold > 1`, it is necessary to
gather a quorum of signatures to update the Radicle identity. To do
this, we use the `rad id` command.

Let's add Bob as a delegate using their DID
`did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn`.

```
$ rad id edit --title "Add Bob" --description "Add Bob as a delegate" --delegates did:key:z6MkedTZGJGqgQ2py2b8kGecfxdt2yRdHWF6JpaZC47fovFn --no-confirm
✓ Identity proposal '0d396a83a5e1dda2b8929f7dc401d19dd1a79fb8' created
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

    👎 no

Let's see what happens when we reject the change:

```
$ rad id reject 0d396a83a5e1dda2b8929f7dc401d19dd1a79fb8 --no-confirm
✓ Rejected proposal 👎
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
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]

Quorum Reached

👎 no
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
$ rad id accept 0d396a --no-confirm
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

Our key has changed from the `Rejected` set to the `Accepted` set
instead:

    Accepted

    total: 1
    keys: [
      "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
    ]

As well as that, the `Quorum` has now been reached:

    Quorum Reached

    👍 yes

At this point, we can commit the proposal and update the identity:

```
$ rad id commit 0d396a83a5e1dda2b8929f7dc401d19dd1a79fb8 --no-confirm
✓ Committed new identity 'c96e764965aaeff1c6ea3e5b97e2b9828773c8b0'
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

Let's say we decide to also change the `threshold`, we can do so using
the `--threshold` option:

```
$ rad id edit --title "Update threshold" --description "Update to safer threshold" --threshold 2 --no-confirm
✓ Identity proposal 'f435d6e89c8f922ede691287c0d8b7f82afa591e' created
title: Update threshold
description: Update to safer threshold
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

👎 no
```

But we change our minds and decide to close the proposal instead:

```
$ rad id close f435d6e89c8f922ede691287c0d8b7f82afa591e --no-confirm
✓ Closed identity proposal 'f435d6e89c8f922ede691287c0d8b7f82afa591e'
title: Update threshold
description: Update to safer threshold
status: ❲closed❳
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

👎 no
```

The proposal is now closed and cannot be committed. If at a later date
we want to update the document with the same change we have to open a
new proposal.

If at any time we want to see what proposals have been made to this
Radicle identity, then we can use the list command:

```
$ rad id list
0d396a83a5e1dda2b8929f7dc401d19dd1a79fb8 "Add Bob"          ❲committed❳
f435d6e89c8f922ede691287c0d8b7f82afa591e "Update threshold" ❲closed❳
```

And if we want to view the latest state of any proposal we can use the
show command:

```
$ rad id show f435d6e89c8f922ede691287c0d8b7f82afa591e
title: Update threshold
description: Update to safer threshold
status: ❲closed❳
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

👎 no
```

On a final note, these examples used `--no-confirm`. The default mode
for making proposals is to select and confirm any actions being
performed on the proposal.

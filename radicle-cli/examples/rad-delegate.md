Delegates are the authorized keys that can manage a project's
metadata, including adding a new delegate.

Let's list the current set of delegates for a project.

```
$ rad delegate list rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
[
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
]
```

We want to add a new maintainer to the project to help out with the
work.

```
$ rad delegate add did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG --to rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Added delegate 'did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG'
✓ Update successful!
```

Let's convince ourselves that there's another delegate.

```
$ rad delegate list rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
[
  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"
]
```

And finally, we no longer want to be part of the project so we pass on
the torch and remove ourselves from the delegate set.

```
$ rad delegate remove did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --to rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Removed delegate 'did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
✓ Update successful!
```

```
$ rad delegate list rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
[
  "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"
]
```

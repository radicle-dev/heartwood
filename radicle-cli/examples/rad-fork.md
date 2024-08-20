If we have fetched a project, then we do not have a fork of the
repository in the storage, i.e. there is no ref hierarchy for our
NID. This is demonstrated below where our NID is
`z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk`:

```
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.id
    │       └── [...]
    ├── heads
    │   └── master
    └── rad
        ├── id
        ├── root
        └── sigrefs
```

To remedy this, we can use the `rad fork` command for the project we
wish to fork:

```
$ rad fork rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Forked repository rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

Now, if we `rad inspect` the project's refs again we will see that we
have a copy of the main set of refs:

```
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.id
    │       └── [...]
    ├── heads
    │   └── master
    └── rad
        ├── id
        ├── root
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── heads
    │   └── master
    └── rad
        ├── root
        └── sigrefs
```

We are now able to setup a remote in our own working copy of the
project and push to our own fork.

If we have fetched a project into storage and have an existing working
copy, then we still do not have a namespace in the stored repository,
i.e. there is no ref hierarchy for our NID. This is demonstrated below
where our NID is
`z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk`:

```
$ rad inspect --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --refs
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

To remedy this, we can push the default branch to our namespace:

```
$ git push rad master
```

Now, if we `rad inspect` the project's refs again we will see that we
have a copy of the main set of refs:

```
$ rad inspect --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --refs
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

We can now continue pushing changes from our working copy to our own
namespace.

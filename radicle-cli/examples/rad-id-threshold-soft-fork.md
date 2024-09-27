In some cases, a peer can create references, which includes `rad/sigrefs`,
without having pushed the canonical default branch. For example, Bob can create
an issue in the repository:

``` ~bob
$ rad issue open --title "Add Bob as a delegate" --description "We agreed to add me as a delegate, so I am creating an issue to track that work" --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
╭──────────────────────────────────────────────────────────────╮
│ Title   Add Bob as a delegate                                │
│ Issue   f12d512c51d30429f7916db038ae0360e2e938c2             │
│ Author  bob (you)                                            │
│ Status  open                                                 │
│                                                              │
│ We agreed to add me as a delegate, so I am creating an issue │
│ to track that work                                           │
╰──────────────────────────────────────────────────────────────╯
✓ Synced with 1 node(s)
```

and if we inspect Alice's references, then we will see the following:

``` ~alice
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.id
    │       └── 0656c217f917c3e06234771e9ecae53aba5e173e
    ├── heads
    │   └── master
    └── rad
        ├── id
        ├── root
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── cobs
    │   └── xyz.radicle.issue
    │       └── f12d512c51d30429f7916db038ae0360e2e938c2
    └── rad
        ├── root
        └── sigrefs
```

Despite not having the canonical branch, Alice should still be able to add Bob
as a delegate, since a threshold of 1 can still be reached:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
7be665f9fccba97abb21b2fa85a6fd3181c72858
```

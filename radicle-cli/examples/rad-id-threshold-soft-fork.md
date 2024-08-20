In some cases, a peer can create references, which includes `rad/sigrefs`,
without having pushed the canonical default branch. For example, Bob can create
an issue in the repository:

``` ~bob
$ rad issue open --title "Add Bob as a delegate" --description "We agreed to add me as a delegate, so I am creating an issue to track that work" --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
╭──────────────────────────────────────────────────────────────╮
│ Title   Add Bob as a delegate                                │
│ Issue   9c0484f7b773477d41787d9b0cf772c741b7b4e4             │
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
    │       └── eeb8b44890570ccf85db7f3cb2a475100a27408a
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
    │       └── 9c0484f7b773477d41787d9b0cf772c741b7b4e4
    └── rad
        ├── root
        └── sigrefs
```

Despite not having the canonical branch, Alice should still be able to add Bob
as a delegate, since a threshold of 1 can still be reached:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
ba5c358894e0a58dd0772fd3eb6d070282dffc26
```

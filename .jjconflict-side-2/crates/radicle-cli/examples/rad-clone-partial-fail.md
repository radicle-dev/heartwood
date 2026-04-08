Eve knows about three seeds.

```
$ rad node routing
╭──────────────────────────────────────────────────────────────────────────────────────╮
│ RID                                 NID                                              │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6MksFqXN3Yhqk8pTJdUGLwBTkRfQvwZXPqR2qMEhbS9wzpT │
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk │
╰──────────────────────────────────────────────────────────────────────────────────────╯
```

When she tries to clone, one of those will fail to fetch. But the clone command
still returns successfully.

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --timeout 3s
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 3 potential seed(s).
✓ Target met: 1 seed(s)
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
```

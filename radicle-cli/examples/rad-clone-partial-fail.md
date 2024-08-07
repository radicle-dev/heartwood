Eve knows about three seeds.

```
$ rad node routing
╭─────────────────────────────────────────────────────╮
│ RID                                 NID             │
├─────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6MknSL…StBU8Vi │
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6MksFq…bS9wzpT │
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6Mkt67…v4N1tRk │
╰─────────────────────────────────────────────────────╯
```
When she tries to clone, one of those will fail to fetch. But the clone command
still returns successfully.

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --timeout 3
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
✗ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk.. error: failed to perform fetch handshake
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✗ Connecting to z6MksFq…bS9wzpT@[..].. error: connection reset
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
```

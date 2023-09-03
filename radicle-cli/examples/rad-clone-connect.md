If we're not connecting to seed nodes when cloning, the `clone` command will
automatically connect to the necessary seeds.

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Tracking relationship established for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
✓ Connecting to z6MknSL…StBU8Vi@[..]
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Connecting to z6Mkt67…v4N1tRk@[..]
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Forking under z6Mkux1…nVhib7Z..
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the project directory.
```

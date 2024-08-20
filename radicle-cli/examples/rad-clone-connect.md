If we're not connecting to seed nodes when cloning, the `clone` command will
automatically connect to the necessary seeds.

```
$ rad clone rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Seeding policy updated for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 with scope 'all'
✓ Connecting to z6Mkt67…v4N1tRk@[..]
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkt67…v4N1tRk@[..]..
✓ Connecting to z6MknSL…StBU8Vi@[..]
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
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

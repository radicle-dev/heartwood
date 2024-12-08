We can specify where a repository gets cloned into on our filesystem
by specifying the directory in the `rad clone` command:

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope followed Developer/Radicle
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi@[..]..
✓ Creating checkout in ./Developer/Radicle..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [..]/Developer/Radicle/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd [..]/Developer/Radicle` to go to the repository directory.
```

Note that attempting to clone into a directory that already exists,
and is not empty, will fail:

``` (fail)
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope followed Developer/Radicle
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi@[..]..
✗ Error: the directory path "Developer/Radicle" already exists
```

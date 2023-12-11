```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope all
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
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

We can now have a look at the new working copy that was created from the cloned
repository:

```
$ cd heartwood
$ cat README
Hello World!
```

Let's check that we have Bob and Alice's namespaces in storage:

```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.id
    │       └── [...]
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── heads
    │   └── master
    └── rad
        └── sigrefs
```

We can then setup a git remote for `bob`:

```
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
```

And fetch his refs:

```
$ git fetch --all
Fetching rad
Fetching alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Fetching bob
$ git branch --remotes
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
  bob/master
  rad/master
```

We can also create our own fork just by pushing:

``` (stderr)
$ git push -o no-sync rad master
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
 * [new branch]      master -> master
```
```
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   └── xyz.radicle.id
    │       └── [...]
    ├── heads
    │   └── master
    └── rad
        ├── id
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── heads
    │   └── master
    └── rad
        └── sigrefs
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
└── refs
    ├── heads
    │   └── master
    └── rad
        └── sigrefs
```

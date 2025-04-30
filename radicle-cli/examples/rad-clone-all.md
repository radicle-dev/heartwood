```
$ rad clone rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --scope all
✓ Seeding policy updated for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 with scope 'all'
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

We can then setup a git remote for `bob`:

```
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --no-sync --fetch
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
$ git branch --remotes
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
  bob/master
  rad/master
```

We can also create our own fork just by pushing:

``` (stderr)
$ git push -o no-sync rad master
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
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
        ├── root
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── heads
    │   └── master
    └── rad
        ├── root
        └── sigrefs
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
└── refs
    ├── heads
    │   └── master
    └── rad
        ├── root
        └── sigrefs
```

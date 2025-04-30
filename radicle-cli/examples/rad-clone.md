To create a local copy of a repository on the radicle network, we use the
`clone` command, followed by the identifier or *RID* of the repository:

```
$ rad clone rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --scope followed
✓ Seeding policy updated for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 with scope 'followed'
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
$ ls
README
$ cat README
Hello World!
$ git status
On branch master
Your branch is up to date with 'rad/master'.

nothing to commit, working tree clean
```

Let's check that the remote tracking branch was setup correctly:

```
$ git branch --remotes
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
  rad/master
```

The first branch is ours, and the second points to the repository delegate.
We can also take a look at the remotes:

```
$ git remote -v
alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi	rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (fetch)
alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi	rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (push)
rad	rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 (fetch)
rad	rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (push)
```

Let's check the last commit!

```
$ git log -n 1
commit f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
Author: anonymous <anonymous@radicle.xyz>
Date:   Mon Jan 1 14:39:16 2018 +0000

    Second commit
```

Cloned repositories show up in `rad ls`:
```
$ rad ls --seeded
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

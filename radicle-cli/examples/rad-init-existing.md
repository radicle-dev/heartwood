Let's clone a regular repository via plain Git:
```
$ git clone $URL heartwood
$ cd heartwood
$ git rev-parse HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

We can see it's not a Radicle working copy:
``` (fail)
$ rad .
✗ Error: Current directory is not a Radicle repository
```

Let's pick an existing repository:
```
$ rad inspect rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

And initialize this working copy as that existing repository:
```
$ rad init --existing rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Initialized existing repository rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 in [..]/heartwood/..
```

We can confirm that the working copy is initialized:
```
$ rad .
rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
$ git remote show rad
* remote rad
  Fetch URL: rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
  Push  URL: rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
  HEAD branch: (unknown)
  Remote branch:
    master new (next fetch will store in remotes/rad)
  Local ref configured for 'git push':
    master pushes to master (up to date)
```

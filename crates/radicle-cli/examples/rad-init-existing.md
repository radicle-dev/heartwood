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
$ rad inspect rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

And initialize this working copy as that existing repository:
```
$ rad init --existing rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Initialized existing repository rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji in [..]/heartwood/..
```

We can confirm that the working copy is initialized:
```
$ rad .
rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
$ git remote show rad
* remote rad
  Fetch URL: rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
  Push  URL: rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
  HEAD branch: (unknown)
  Remote branch:
    master new (next fetch will store in remotes/rad)
  Local ref configured for 'git push':
    master pushes to master (up to date)
```

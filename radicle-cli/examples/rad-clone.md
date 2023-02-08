To create a local copy of a repository on the radicle network, we use the
`clone` command, followed by the identifier or *RID* of the repository:

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
ok Tracking relationship established for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
ok Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
ok Forking under z6Mkt67…v4N1tRk..
ok Creating checkout in ./heartwood..
ok Remote z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi created
ok Remote-tracking branch z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi

🌱 Project successfully cloned under [..]/heartwood/

```

We can now have a look at the new working copy that was created from the cloned
repository:

```
$ cd heartwood
$ ls
README
$ cat README
Hello World!
```

Let's check that the remote tracking branch was setup correctly:

```
$ git branch --remotes
  rad/master
  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

The first branch is ours, and the second points to the repository delegate.
We can also take a look at the remotes:

```
$ git remote -v
rad	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (fetch)
rad	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (push)
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (fetch)
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (push)
```

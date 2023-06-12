Now, let's add a bob as a new remote:

```
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
```

Now, we can see that there is a new remote in the list of remotes:

```
$ rad remote list
bob z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (fetch)
rad (canonical upstream)                             (fetch)
rad z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (push)
```

You can see both `bob` and `rad` as remotes.  The `rad` remote is our personal
remote of the project.

For the remote-tracking branch to work, we fetch bob:

``` (stderr)
$ git fetch bob
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      master     -> bob/master
```

We can now see the remote-tracking branch that was setup:

```
$ git branch -r -v
  bob/master f2de534 Second commit
  rad/master f2de534 Second commit
```

When we're finished with the `bob` remote, we can remove it:

```
$ rad remote rm bob
✓ Remote `bob` removed
$ git branch -r -v
  rad/master f2de534 Second commit
```

Now, add another time `bob` but without specify the `name`, so we should be
able to fetch the node alias from our db!

```
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
```

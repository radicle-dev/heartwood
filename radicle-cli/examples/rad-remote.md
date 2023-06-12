Now, let's add a bob as a new remote:

```
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Remote bob added
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

When we're finished with the `bob` remote, we can remove it:

```
$ rad remote rm bob
✓ Remote `bob` removed
```

Now, add another time `bob` but without specify the `name`, so we should be
able to fetch the node alias from our db!

```
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Remote bob added
```

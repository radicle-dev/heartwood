Now, let's add a bob as a new remote:

```
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --no-sync
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
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-sync
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
```

We can also use `rad remote` to list all the remotes that are
available in the repository by using the `--untracked` flag:

```
$ rad remote --untracked
eve did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
```

If we use `--all`, then we can see all the remotes that we have
created in the working copy, followed by all the available remotes:

```
$ rad remote --all
bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (fetch)
rad                                                  (canonical upstream)                             (fetch)
rad                                                  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (push)

eve did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
```

As we can see, we have also have another remote namespace `eve`, so
let's add them to our set of working copy remotes:

```
$ rad remote add did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --name eve --no-sync
✓ Remote eve added
✓ Remote-tracking branch eve/master created for z6Mkux1…nVhib7Z
```

After adding `eve`'s remote, we no longer see any entries that are
untracked:

```
$ rad remote --all
bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (fetch)
eve                                                  z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z (fetch)
rad                                                  (canonical upstream)                             (fetch)
rad                                                  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (push)
```


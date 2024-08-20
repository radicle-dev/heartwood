We cannot delete a repository, since that can cause data integrity
issues. However, we can clean the storage of remotes that are not the
local peer or the repository delegates. To do this we can use the `rad
clean` command.

First let's look at what we have locally:

``` ~alice
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Let's also inspect what remotes are in the repository:

``` ~alice
$ rad inspect --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 7c1445cd018b1b0f51e0d815c3c03b289140eafa
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk 2694db11d1ce3fb21f4cee6840f7daa6846366b2
```

Now let's clean the `heartwood` project:

``` ~alice
$ rad clean rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --no-confirm
Removed z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Successfully cleaned rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

Inspecting the remotes again, we see that Bob is now gone:

``` ~alice
$ rad inspect --sigrefs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 7c1445cd018b1b0f51e0d815c3c03b289140eafa
```

Note that Bob will be fetched again if we do not untrack his
node. Currently, there is no per repository tracking so it's not
possible to stop fetching Bob for this particular repository.

Cleaning a repository again will remove no remotes, since we're
already at the minimal set of remotes:

``` ~alice
$ rad clean rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --no-confirm
✓ Successfully cleaned rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

Since Eve did not fork the repository, and has no refs of their own,
when they run `rad clean` it will remove the project entirely:

``` ~eve
$ rad clean rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --no-confirm
Removed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
✓ Successfully cleaned rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

And attempting to clean the repository again, or any non-existent
repository, has no effect on the storage at all:

``` ~eve (fail)
$ rad clean rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --no-confirm
✗ Error: repository rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 was not found
```

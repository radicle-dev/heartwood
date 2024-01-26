The `rad sync` command announces changes to the network and waits for other
nodes to be synchronized with those changes.

For instance let's create an issue and sync it with the network:

```
$ rad issue open --title "Test `rad sync`" --description "Check that the command works" -q --no-announce
```

If we check the sync status, we see that our peers are out of sync.

```
$ rad sync status --sort-by alias
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                  Status        Tip       Timestamp │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.xyz:8776                 f209c9f   [  ...  ] │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.xyz:8776     out-of-sync   f209c9f   [  ...  ] │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.xyz:8776     out-of-sync   f209c9f   [  ...  ] │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

Now let's run `rad sync`. This will announce the issue refs to the network and
wait for nodes to announce that they have fetched those refs.

```
$ rad sync --announce
✓ Synced with 2 node(s)
```

Now, when we run `rad sync status` again, we can see that `bob` and
`eve` are up-to-date:

```
$ rad sync status --sort-by alias
╭─────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                  Status   Tip       Timestamp │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.xyz:8776            9f615f9   [  ...  ] │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.xyz:8776     synced   9f615f9   [  ...  ] │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.xyz:8776     synced   9f615f9   [  ...  ] │
╰─────────────────────────────────────────────────────────────────────────────────────╯
```

If we try to sync again after the nodes have synced, we will already
be up to date.

```
$ rad sync --announce
✓ Nothing to announce, already in sync with 2 node(s) (see `rad sync status`)
```

We can also use the `--fetch` option to only fetch objects:

```
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetched repository from 2 seed(s)
```

Specifying both `--fetch` and `--announce` is equivalent to specifying none:

```
$ rad sync --fetch --announce
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetched repository from 2 seed(s)
✓ Nothing to announce, already in sync with 2 node(s) (see `rad sync status`)
```

It's also possible to use the `--seed` flag to only sync with a specific node:

```
$ rad sync --fetch --seed z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
```

And the `--replicas` flag to sync with a number of nodes. First we'll
create a new issue so that we have something to announce:

```
$ rad issue open --title "Test `rad sync --replicas`" --description "Check that the replicas works" -q --no-announce
```

```
$ rad sync --replicas 1
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
✓ Synced with 1 node(s)
```

Note that we see `✓ Fetched repository from 1 seed(s)` and `✓ Synced
with 1 node(s)`. This does not necessarily mean that only `bob` or
`eve` were synchronized with, since they both could have received the
announcement of the new changes. However, it does mean that we only
wait for at least 1 of the nodes to have fetched the changes from us.


It's also possible to receive an error if a repository is not found anywhere.

```
$ rad seed rad:z39mP9rQAaGmERfUMPULfPUi473tY
✓ Seeding policy updated for rad:z39mP9rQAaGmERfUMPULfPUi473tY with scope 'all'
```
``` (fail)
$ rad sync rad:z39mP9rQAaGmERfUMPULfPUi473tY
✗ Error: no seeds found for rad:z39mP9rQAaGmERfUMPULfPUi473tY
✗ Error: nothing to announce, repository rad:z39mP9rQAaGmERfUMPULfPUi473tY is not available locally
```

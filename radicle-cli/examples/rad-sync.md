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

If we try to sync again after the nodes have synced, we will already
be up to date.

```
$ rad sync --announce
✓ Nothing to announce, already in sync with network (see `rad sync status`)
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
✓ Nothing to announce, already in sync with network (see `rad sync status`)
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

We can check the sync status again to make sure everything's in sync:

```
$ rad sync status --sort-by alias
╭─────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                  Status   Tip       Timestamp │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.xyz:8776            9ee4e68   [  ...  ] │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.xyz:8776     synced   9ee4e68   [  ...  ] │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.xyz:8776     synced   9ee4e68   [  ...  ] │
╰─────────────────────────────────────────────────────────────────────────────────────╯
```

Note that both `bob` and `eve` are in sync despite seeing `Synced with
1 node(s)`. This is because the announcement was still made to `bob`
and `eve`, but we will only wait for at least `1` replica to be
synchronized.

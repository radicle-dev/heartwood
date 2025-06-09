The `rad sync` command announces changes to the network and waits for other
nodes to be synchronized with those changes.

For instance let's create an issue and sync it with the network:

```
$ rad issue open --title "Test `rad sync`" --description "Check that the command works" -q --no-announce
```

If we check the sync status, we see that our peers are out of sync, and our
change has not yet been announced.

```
$ rad sync status --sort-by alias
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                      Status        Tip       Timestamp │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.example:8776   unannounced   056b1db   [  ...  ] │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.example:8776     out-of-sync   99c5497   [  ...  ] │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.example:8776     out-of-sync   99c5497   [  ...  ] │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```

Now let's run `rad sync`. This will announce the issue refs to the network and
wait for nodes to announce that they have fetched those refs.

```
$ rad sync --announce
✓ Synced with 2 seed(s)
```

Now, when we run `rad sync status` again, we can see that `bob` and
`eve` are up-to-date:

```
$ rad sync status --sort-by alias
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   Node                      Address                      Status   Tip       Timestamp │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   alice   (you)             alice.radicle.example:8776            056b1db   [  ...  ] │
│ ●   bob     z6Mkt67…v4N1tRk   bob.radicle.example:8776     synced   056b1db   [  ...  ] │
│ ●   eve     z6Mkux1…nVhib7Z   eve.radicle.example:8776     synced   056b1db   [  ...  ] │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

If we try to sync again after the nodes have synced, we will already
be up to date.

```
$ rad sync --announce
✓ Nothing to announce, already in sync with 2 seed(s) (see `rad sync status`)
```

We can also use the `--fetch` option to only fetch objects:

```
$ rad sync --fetch
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 seed(s)
🌱 Fetched from z6Mkux1…nVhib7Z
🌱 Fetched from z6Mkt67…v4N1tRk
```

Specifying both `--fetch` and `--announce` is equivalent to specifying none:

```
$ rad sync --fetch --announce
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 2 seed(s)
🌱 Fetched from z6Mkux1…nVhib7Z
🌱 Fetched from z6Mkt67…v4N1tRk
✓ Nothing to announce, already in sync with 2 seed(s) (see `rad sync status`)
```

It's also possible to use the `--seed` flag to only sync with a specific node:

```
$ rad sync --fetch --seed z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 3 potential seed(s).
✓ Target met: 1 preferred seed(s).
🌱 Fetched from z6Mkt67…v4N1tRk
```

And the `--replicas` flag to sync with a number of nodes. First we'll
create a new issue so that we have something to announce:

```
$ rad issue open --title "Test `rad sync --replicas`" --description "Check that the replicas works" -q --no-announce
```

```
$ rad sync --replicas 1
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 2 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkux1…nVhib7Z
✓ Synced with 1 seed(s)
```

Note that we see `✓ Fetched repository from 1 seed(s)` and `✓ Synced
with 1 node(s)`. This does not necessarily mean that only `bob` or
`eve` were synchronized with, since they both could have received the
announcement of the new changes. However, it does mean that we only
wait for at least 1 of the nodes to have fetched the changes from us.


It's also possible to receive an error if a repository is not found anywhere.

```
$ rad seed rad:z39mP9rQAaGmERfUMPULfPUi473tY --no-fetch
✓ Seeding policy updated for rad:z39mP9rQAaGmERfUMPULfPUi473tY with scope 'all'
```
``` (fail)
$ rad sync rad:z39mP9rQAaGmERfUMPULfPUi473tY
✗ Error: no candidate seeds were found to fetch from
```

Or when trying to fetch from an unknown seed, using `--seed`:
```
$ rad sync --fetch rad:z39mP9rQAaGmERfUMPULfPUi473tY --seed z6MkjM3HpqNVV4ZsL5s3RAd8ThVG3VG98YsDCjHBNnGMq5o7
Fetching rad:z39mP9rQAaGmERfUMPULfPUi473tY from the network, found 1 potential seed(s).
✗ Target not met: could not fetch from [z6MkjM3…nGMq5o7], and required 1 more seed(s)
✗ Error: Fetched from 0 preferred seed(s), could not reach 1 seed(s)
✗ Error: Could not replicate from 1 preferred seed(s)
✗ Error: z6MkjM3…nGMq5o7: Could not connect. No addresses known.
```

Also note that you cannot sync an unseeded repo:
```
$ rad unseed rad:z39mP9rQAaGmERfUMPULfPUi473tY
[...]
```
``` (fail)
$ rad sync rad:z39mP9rQAaGmERfUMPULfPUi473tY
✗ Error: repository rad:z39mP9rQAaGmERfUMPULfPUi473tY is not seeded
```

The radicle node is our daemon friend that is running as a background
process. It allows us to interact with the network as well as storing
some key data that we may be interested in.

If the node is not running we can start it by using the `rad node
start` command:

```
$ rad node start
✓ Node is already running.
```

We can confirm the status of the node at any time by using the `rad
node status` command (or just `rad node` for short):

```
$ rad node status
✓ Node is running and listening on [..].
```

```
$ rad node config --addresses
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi@41.12.98.112:8776
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi@seed.cloudhead.io:8776
```

The node also allows us to query data that it has access to such as
the follow policies and the routing table. Before we explore
those commands we'll first follow a peer so that we have something to
see.

```
$ rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias Bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (Bob)
```

Now, when we use the `rad seed` command we will see information for
repositories that we seed -- in this case a repository that was already
created:

```
$ rad seed
╭───────────────────────────────────────────────────────────────────╮
│ Repository                          Name        Policy   Scope    │
├───────────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   heartwood   allow    followed │
╰───────────────────────────────────────────────────────────────────╯
```

If we wish to see which nodes we are following:

```
$ rad follow
╭───────────────────────────────────────────────────────────────────────────╮
│ DID                                                        Alias   Policy │
├───────────────────────────────────────────────────────────────────────────┤
│ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk   Bob     allow  │
╰───────────────────────────────────────────────────────────────────────────╯
```

To see the routing table we can use the `rad node routing` command and
see what Repository IDs match up with the interests of which Node
IDs. In this case, it is just our own Node ID for the project we
created.

```
$ rad node routing
╭─────────────────────────────────────────────────────╮
│ RID                                 NID             │
├─────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6MknSL…StBU8Vi │
╰─────────────────────────────────────────────────────╯
```

Finally, if we want to stop the daemon process from running we can
issue the `rad node stop` command:

```
$ rad node stop
✓ Node stopped
```

Running the command again gives us an error:

```
$ rad node stop
✗ Stopping node... error: node is not running
```

Note that if we unseed a repository, it is no longer part of our inventory:

```
$ rad unseed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji removed
$ rad node routing --nid z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Likewise, if we seed a repository we don't have locally, it won't show up as
part of our inventory:
```
$ rad seed rad:z3trNYnLWS11cJWC6BbxDs5niGo82
[...]
$ rad node routing --nid z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

But if we start seeding the repository we have locally again, it'll show
up in our inventory:
```
$ rad seed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
[...]
$ rad node routing --nid z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
╭─────────────────────────────────────────────────────╮
│ RID                                 NID             │
├─────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   z6MknSL…StBU8Vi │
╰─────────────────────────────────────────────────────╯
```

Some commands also give us a hint if the node isn't running:

``` (fail)
$ rad node events
✗ Error: failed to open node control socket "[..]/.radicle/node/control.sock" (entity not found)
✗ Hint: to start your node, run `rad node start`.
```
``` (fail)
$ rad sync status
✗ Error: to sync a repository, your node must be running. To start it, run `rad node start`
```
``` (fail)
$ rad node connect z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk@radicle.xyz:8776
✗ Connecting to z6Mkt67…v4N1tRk@radicle.xyz:8776... <canceled>
✗ Error: failed to open node control socket "[..]/.radicle/node/control.sock" (entity not found)
✗ Hint: to start your node, run `rad node start`.
```

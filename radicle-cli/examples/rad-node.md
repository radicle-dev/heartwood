The radicle node is our daemon friend that is running as a background
process. It allows us to interact with the network as well as storing
some key data that we may be interested in.

<details>
<summary>🚧 `rad node start` is under construction 🚧</summary>
If the node is not running we can start it by using the `rad node
start` command:

<!-- ``` -->
<!-- $ rad node start -->
<!-- ``` -->
</details>

We can confirm the status of the node at any time by using the `rad
node status` command (or just `rad node` for short):

```
$ rad node status
✓ The node is running
```

The node also allows us to connect with other nodes in the
network. For example, let's connect to a known peer using their Node
ID and their network address:

```
$ rad node connect z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk 0.0.0.0:3679
✓ Connecting to z6Mkt67…v4N1tRk@0.0.0.0:3679...
```

The node also allows us to query data that it has access too such as
the tracking relationships and the routing table. Before we explore
those commands we'll first track a peer so that we have something to
see.

```
$ rad track z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias Bob
Establishing 🌱 tracking relationship for heartwood

✓ Tracking relationship with Bob (z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk) established
! Warning: fetch after track is not yet supported
```

Now, when we use the `rad node tracking` command we will see
information for repositories that we track -- in this case a
repository that was already created:

```
$ rad node tracking
RID                               Scope   Policy
---                               -----   ------
rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji trusted track
```

This is the same as using the `--repos` flag, but if we wish to see
which nodes we are specifically tracking, then we use the `--nodes`
flag:

```
$ rad node tracking --nodes
DID                                                      Alias Policy
---                                                      ----- ------
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk Bob   track
```

To see the routing table we can use the `rad node routing` command and
see what Repository IDs match up with the interests of which Node
IDs. In this case, it is just our own Node ID for the project we
created.

```
$ rad node routing
RID                               NID
---                               ---
rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji z6MknSL…StBU8Vi
```

<details>
<summary>🚧 `rad node stop` is under construction 🚧</summary>
Finally, if we want to stop the daemon process from running we can
issue the `rad node stop` command:

<!-- ``` -->
<!-- $ rad node stop -->
<!-- Stopping the node... -->
<!-- ``` -->
</details>

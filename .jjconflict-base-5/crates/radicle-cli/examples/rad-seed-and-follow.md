To configure our node's seeding and follow policy, we can use the `rad seed`
and `rad follow` commands.
For example, let's follow a remote node we know about, and alias it to "eve":

```
$ rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias eve
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (eve)
```

We can list the followed peers by omitting the DID:

```
$ rad follow
╭───────────────────────────────────────────────────────────────────────────╮
│ DID                                                        Alias   Policy │
├───────────────────────────────────────────────────────────────────────────┤
│ did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk   eve     allow  │
╰───────────────────────────────────────────────────────────────────────────╯
```

Now let's seed one of Eve's repositories:

```
$ rad seed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope followed --no-fetch
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
```

We can list the repositories we are seeding by omitting the RID:

```
$ rad seed
╭──────────────────────────────────────────────────────────────╮
│ Repository                          Name   Policy   Scope    │
├──────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji          allow    followed │
╰──────────────────────────────────────────────────────────────╯
```

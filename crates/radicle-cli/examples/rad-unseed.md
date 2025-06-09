Let's say we have a local repository we've initialized:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

We could stop seeding it if we didn't want other nodes to fetch it from us:

```
$ rad unseed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji removed
```

Now, if we run `rad ls`, we see it's gone:

```
$ rad ls
Nothing to show.
$ rad ls --seeded
Nothing to show.
```

However, with the `--all` flag, we can see it still, but as local-only:

```
$ rad ls --all
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   local        f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Hence, we also see that it isn't in our inventory and isn't seeded:

```
$ rad node inventory
$ rad seed
No seeding policies to show.
```

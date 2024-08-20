Let's say we have a local repository we've initialized:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

We could stop seeding it if we didn't want other nodes to fetch it from us:

```
$ rad unseed rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
✓ Seeding policy for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 removed
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
│ heartwood   rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2   local        f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Hence, we also see that it isn't in our inventory and isn't seeded:

```
$ rad node inventory
$ rad seed
No seeding policies to show.
```

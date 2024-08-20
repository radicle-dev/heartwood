It is possible to use the `rad unseed` command to specify multiple RIDs at the
same time, where each repository specified will stop being seeded.

Let's say we have multiple local repositories we've initialized:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2   public       f2de534   Radicle Heartwood Protocol & Stack │
│ nixpkgs     rad:z3rK5Ldp958XdzwL88vYRvhdQj5WR   public       f2de534   Home for Nix Packages              │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

We could stop seeding them if we didn't want other nodes to fetch them from us:

```
$ rad unseed rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 rad:z3rK5Ldp958XdzwL88vYRvhdQj5WR
✓ Seeding policy for rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 removed
✓ Seeding policy for rad:z3rK5Ldp958XdzwL88vYRvhdQj5WR removed
```

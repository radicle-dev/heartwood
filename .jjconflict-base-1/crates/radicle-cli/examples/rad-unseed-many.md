It is possible to use the `rad unseed` command to specify multiple RIDs at the
same time, where each repository specified will stop being seeded.

Let's say we have multiple local repositories we've initialized:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   public       f2de534   Radicle Heartwood Protocol & Stack │
│ nixpkgs     rad:zyFFr2iwoTEfNF4jGNZHuoy7odMh    public       f2de534   Home for Nix Packages              │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

We could stop seeding them if we didn't want other nodes to fetch them from us:

```
$ rad unseed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji rad:zyFFr2iwoTEfNF4jGNZHuoy7odMh
✓ Seeding policy for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji removed
✓ Seeding policy for rad:zyFFr2iwoTEfNF4jGNZHuoy7odMh removed
```

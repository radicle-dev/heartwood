
To create your first Radicle repository, navigate to a git repository, and run the
`init` command.  Make sure you have [authenticated](../rad-auth.md) beforehand.

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm --public -v

Initializing public Radicle 👾 repository in [..]

✓ Repository heartwood created.
{
  "name": "heartwood",
  "description": "Radicle Heartwood Protocol & Stack",
  "defaultBranch": "master"
}

Your Repository ID (RID) is rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji.
You can show it any time by running `rad .` from this directory.

Your repository will be announced to the network when you start your node.
You can start your node with `rad node start`.
To push changes, run `git push`.
```

If we try to initialize it again, we get an error:

``` (fail)
$ rad init
✗ Error: repository is already initialized with remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

Repositories can be listed with the `ls` command:

```
$ rad ls
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   public       f2de534   Radicle Heartwood Protocol & Stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Public repositories are added to our inventory:

```
$ rad node inventory
rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```


To create your first radicle project, navigate to a git repository, and run
the `init` command:

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm --no-sync --no-track

Initializing local 🌱 project in .

ok Project heartwood created
{
  "name": "heartwood",
  "description": "Radicle Heartwood Protocol & Stack",
  "defaultBranch": "master"
}

Your project id is rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji. You can show it any time by running:
    rad .

To publish your project to the network, run:
    rad push

```

Projects can be listed with the `ls` command:

```
$ rad ls
heartwood rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji f2de534 Radicle Heartwood Protocol & Stack
```


To create your first radicle project, navigate to a git repository, and run
the `init` command:

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm

Initializing local 🌱 project in .

ok Project heartwood created
{
  "name": "heartwood",
  "description": "Radicle Heartwood Protocol & Stack",
  "defaultBranch": "master"
}


Your project id is rad:z2TBtGrJKGsremYAPec6vN4n77Ba7. You can show it any time by running:
   rad .

To publish your project to the network, run:
   rad push

```

Projects can be listed with the `ls` command:

```
$ rad ls
heartwood rad:z2TBtGrJKGsremYAPec6vN4n77Ba7 cdf76ce Radicle Heartwood Protocol & Stack
```

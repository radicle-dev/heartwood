
To create your first radicle project, navigate to a git repository, and run
the `init` command:

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm

Initializing local 🌱 project in .

ok Project heartwood created
{
  "name": "heartwood",
  "description": "Radicle Heartwood Protocol & Stack",
  "default-branch": "master"
}


Your project id is rad:zb2rNRYmmz7SLZ7xMjM7dddswC3K. You can show it any time by running:
   rad .

To publish your project to the network, run:
   rad push

```

Projects can be listed with the `ls` command:

```
$ rad ls
heartwood rad:zb2rNRYmmz7SLZ7xMjM7dddswC3K cdf76ce Radicle Heartwood Protocol & Stack
```

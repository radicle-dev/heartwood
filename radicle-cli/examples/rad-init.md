
To create your first radicle project, navigate to a git repository, and run
the `init` command:

```
$ rad init --name acme --description "Acme's repository" --no-confirm

Initializing local 🌱 project in .

ok Project acme created
{
  "name": "acme",
  "description": "Acme's repository",
  "default-branch": "master"
}


Your project id is rad:z4EXAgDJk11uFThfVir5FivKhiAeK. You can show it any time by running:
   rad .

To publish your project to the network, run:
   rad push

```

Projects can be listed with the `ls` command:

```
$ rad ls
acme rad:z4EXAgDJk11uFThfVir5FivKhiAeK cdf76ce Acme's repository
```

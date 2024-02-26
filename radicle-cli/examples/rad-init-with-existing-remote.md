Let's try to `rad init` a repo which already has a remote tracking branch for `master`.

First we have to create a valid remote repository.

```
$ git init --bare remote
Initialized empty Git repository in [..]
```

Then we add it as a remote.

```
$ git remote add origin file://$PWD/remote
$ git push -u origin master:master
branch 'master' set up to track 'origin/master'.
$ git branch -vv
* master f2de534 [origin/master] Second commit
```

Then we initialize.

```
$ rad init --name heartwood --description "Heartwood Protocol & Stack" --no-confirm --public

Initializing public radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z2D6wQnKapY7dn5meBnbH2rUKNZbT.
You can show it any time by running `rad .` from this directory.

Your repository will be announced to the network when you start your node.
You can start your node with `rad node start`.
To push changes, run `git push rad master`.
```

Finally we run the suggested command.

``` (stderr)
$ git push rad master
Everything up-to-date
```

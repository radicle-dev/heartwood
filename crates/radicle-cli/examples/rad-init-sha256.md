Create a Git repository using SHA-256 object IDs:

```
$ git init --object-format=sha256 --initial-branch=master -q
$ git rev-parse --show-object-format
sha256
$ touch README
$ git add README
$ git commit -m "Initial commit" -q
```

Initialize it as a public Radicle repository and sync it to the other node:

```
$ rad init --name sha256 --description "SHA-256 repository" --no-confirm --public --scope followed

Initializing public Radicle 👾 repository in [..]

✓ Repository sha256 created.

Your Repository ID (RID) is rad:[..]
You can show it any time by running `rad .` from this directory.

✓ Repository successfully synced to [..]
✓ Repository successfully synced to 1 node(s).

Your repository has been synced to the network and is now discoverable by peers.
To push changes, run `git push`.
```

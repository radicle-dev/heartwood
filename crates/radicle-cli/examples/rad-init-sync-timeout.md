Sometimes, `init` will fail to sync with the network. This is not a big deal,
as the node will keep attempting to sync in the background.

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm --public --scope followed

Initializing public Radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z3Rry7rpdWuGpfjPYGzdJKQADsoNW.
You can show it any time by running `rad .` from this directory.

✓ Repository successfully announced to the network.

Your repository has been announced to the network and is now discoverable by peers.
You can check for any nodes that have replicated your repository by running `rad sync status`.

To push changes, run `git push`.
```

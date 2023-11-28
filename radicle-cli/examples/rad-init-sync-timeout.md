Sometimes, `init` will fail to sync with the network. This is not a big deal,
as the node will keep attempting to sync in the background.

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm --public --scope followed

Initializing public radicle 👾 project in .

✓ Project heartwood created.

Your project's Repository ID (RID) is rad:z3Rry7rpdWuGpfjPYGzdJKQADsoNW.
You can show it any time by running `rad .` from this directory.

✓ Project successfully announced to the network.

Your project has been announced to the network and is now discoverable by peers.
You can check for any nodes that have replicated your project by running `rad sync status`.

To push changes, run `git push`.
```

When initializing a repository without any peer connections, we get this output:

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm --public --scope followed

Initializing public radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji.
You can show it any time by running `rad .` from this directory.

✗ Announcing.. <canceled>

You are not connected to any peers. Your repository will be announced as soon as your node establishes a connection with the network.
Check for peer connections with `rad node status`.

To push changes, run `git push`.
```

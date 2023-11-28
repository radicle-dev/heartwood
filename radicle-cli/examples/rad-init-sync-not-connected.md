When initializing a project without any peer connections, we get this output:

```
$ rad init --name heartwood --description "Radicle Heartwood Protocol & Stack" --no-confirm --public --scope followed

Initializing public radicle 👾 project in .

✓ Project heartwood created.

Your project's Repository ID (RID) is rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji.
You can show it any time by running `rad .` from this directory.

✗ Announcing.. <canceled>

You are not connected to any peers. Your project will be announced as soon as your node establishes a connection with the network.
Check for peer connections with `rad node status`.

To push changes, run `git push`.
```

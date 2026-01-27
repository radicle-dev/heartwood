If we initialize a public repository without seeding it, it won't be advertised:
```
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --public --no-seed

Initializing public radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK.
You can show it any time by running `rad .` from this directory.

Your repository will be announced to the network when you start your node.
You can start your node with `rad node start`.
To push changes, run `git push`.
```
```
$ rad node inventory
```

If we then seed it, it becomes advertised in our inventory:
```
$ rad seed rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK
✓ Inventory updated with rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK
✓ Seeding policy updated for rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK with scope 'followed'
```
```
$ rad node inventory
rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK
```

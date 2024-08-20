If we initialize a public repository without seeding it, it won't be advertized:
```
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --public --no-seed

Initializing public radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg.
You can show it any time by running `rad .` from this directory.

Your repository will be announced to the network when you start your node.
You can start your node with `rad node start`.
To push changes, run `git push`.
```
```
$ rad node inventory
```

If we then seed it, it becomes advertized in our inventory:
```
$ rad seed rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg
✓ Inventory updated with rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg
✓ Seeding policy updated for rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg with scope 'all'
```
```
$ rad node inventory
rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg
```

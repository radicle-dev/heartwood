Alice can initialize a *private* repo using the `--private` flag.

```
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --private

Initializing private radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT.
You can show it any time by running `rad .` from this directory.

You have created a private repository.
This repository will only be visible to you, and to peers you explicitly allow.

To make it public, run `rad publish`.
To push changes, run `git push`.
```

The repository does not show up in our inventory, since it is not advertized,
despite being seeded:
```
$ rad node inventory
$ rad seed
╭────────────────────────────────────────────────────────────────╮
│ Repository                          Name        Policy   Scope │
├────────────────────────────────────────────────────────────────┤
│ rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT   heartwood   allow    all   │
╰────────────────────────────────────────────────────────────────╯
```

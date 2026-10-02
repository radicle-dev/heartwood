Alice can initialize a *private* repo using the `--private` flag.

```
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --private

Initializing private Radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu
You can show it any time by running `rad .` from this directory.

You have created a private repository.
This repository will only be visible to you, and to peers you explicitly allow.

! Warning: There is a severe vulnerability affecting the network protocol that Radicle uses. See https://radicle.dev/2026/09/23/disclosure-of-vulnerability-in-network-protocol for details. Only use private repositories if you understand the risks and want to take them.

To make it public, run `rad publish`.
To push changes, run `git push`.
```

The repository does not show up in our inventory, since it is not advertised,
despite being seeded:
```
$ rad node inventory
$ rad seed
╭────────────────────────────────────────────────────────────────╮
│ Repository                          Name        Policy   Scope │
├────────────────────────────────────────────────────────────────┤
│ rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu   heartwood   allow    all   │
╰────────────────────────────────────────────────────────────────╯
```

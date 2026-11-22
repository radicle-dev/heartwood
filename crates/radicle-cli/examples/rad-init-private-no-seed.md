Let's say we initialize a private repository and specify that we don't want it
to be seeded. This means that the repo will be available locally, to us,
and even if other peers know about it, they won't be able to fetch it
from us.
```
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --private --no-seed

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

```
$ rad seed
No seeding policies to show.
```

We can decide to seed it later, so that others can fetch it from us, given
that they are part of the allow list:
```
$ rad seed rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu
✓ Seeding policy updated for rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu with scope 'followed'
```

But it still won't show up in our inventory, since it's private:
```
$ rad node inventory
```

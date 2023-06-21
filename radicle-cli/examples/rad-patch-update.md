Let's explore the `rad patch update` plumbing command. First we create a patch:

``` (stderr)
$ git checkout -q -b feature/1
$ git commit -q -m "Not a real change" --allow-empty
```
``` (stderr)
$ git push rad HEAD:refs/patches
✓ Patch 51e0d0bc168ccdc541b7b1aeab2eb9e048c2fcdd opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ rad patch show 51e0d0bc168ccdc541b7b1aeab2eb9e048c2fcdd
╭────────────────────────────────────────────────────────────────────╮
│ Title     Not a real change                                        │
│ Patch     51e0d0bc168ccdc541b7b1aeab2eb9e048c2fcdd                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      51b2f0f77b9849bfaa3e9d3ff68ee2f57771d20c                 │
│ Branches  feature/1                                                │
│ Commits   ahead 1, behind 0                                        │
│ Status    open                                                     │
├────────────────────────────────────────────────────────────────────┤
│ 51b2f0f Not a real change                                          │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) (z6MknSL…StBU8Vi) [           ...              ] │
╰────────────────────────────────────────────────────────────────────╯
```

We can make some changes to the repository:

```
$ git mv README README.md
$ git commit -q -m "Rename readme file"
```

Let's push the changes, but not to the magic ref, that way the push doesn't
update our patch:

```
$ git push rad HEAD:refs/heads/feature/1
```

Now, instead of using `git push` to update the patch, as we normally would,
we run:

```
$ rad patch update 51e0d0bc168ccdc541b7b1aeab2eb9e048c2fcdd -m "Updated patch"
c10012c2cb9c0c9bfeba7ef28cae10e4b8db3469
```

The command outputs the new Revision ID, which we can now see here:

```
$ rad patch show 51e0d0bc168ccdc541b7b1aeab2eb9e048c2fcdd
╭──────────────────────────────────────────────────────────────────────────────╮
│ Title     Not a real change                                                  │
│ Patch     51e0d0bc168ccdc541b7b1aeab2eb9e048c2fcdd                           │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi           │
│ Head      4d272148458a17620541555b1f0905c01658aa9f                           │
│ Branches  feature/1                                                          │
│ Commits   ahead 2, behind 0                                                  │
│ Status    open                                                               │
├──────────────────────────────────────────────────────────────────────────────┤
│ 4d27214 Rename readme file                                                   │
│ 51b2f0f Not a real change                                                    │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) (z6MknSL…StBU8Vi) [                                ...   ] │
│ ↑ updated to c10012c2cb9c0c9bfeba7ef28cae10e4b8db3469 (4d27214) [    ...   ] │
╰──────────────────────────────────────────────────────────────────────────────╯
```

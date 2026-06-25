Sometimes we are a bit forgetful and miss a detail when creating a
patch. In this case we'll stack two patches by creating one after the
other.

First we add a `REQUIREMENTS` file:

```
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 602ba44] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```
``` (stderr)
$ git push rad flux-capacitor-power
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      flux-capacitor-power -> flux-capacitor-power
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 70a5938a2620ffad0cef086670112c65f69a3d48 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

And then a `README` file:
```
$ git checkout -b add-readme
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[add-readme 8083d4c] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```
``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun" HEAD:refs/patches
✓ Patch 07eab0ca9297cbbd586ca46594be1839aea13434 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Our second patch looks like the following:

```
$ rad patch show 07eab0ca9297cbbd586ca46594be1839aea13434
╭──────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                   │
│ Patch     07eab0ca9297cbbd586ca46594be1839aea13434       │
│ Author    alice (you)                                    │
│ Head      8083d4cbb2b297ade6a12962f8ddc118c3900dcf       │
│ Base      4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927       │
│ Target    master                                         │
│ Branches  add-readme                                     │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 8083d4c Add README, just for the fun                     │
│ 602ba44 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision 07eab0c @ 4c66f0e..8083d4c by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

But wait, we meant to stack them and so we don't want to see the
commit `602ba44` as part of this patch, so we create a new revision
with a new `base`:

```
$ rad patch update 07eab0c -b 602ba44 -m "Whoops, forgot to set the base" --no-announce
2cebe5d06baa1a1b4f71c4d3fe5857b274c3c1d5
```

Now, if we show the patch we can see the patch's base has changed and
we have a single commit:

```
$ rad patch show 07eab0c
╭──────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                   │
│ Patch     07eab0ca9297cbbd586ca46594be1839aea13434       │
│ Author    alice (you)                                    │
│ Head      8083d4cbb2b297ade6a12962f8ddc118c3900dcf       │
│ Base      602ba4448210fba26633dc3f9ae3d4d9d20a1e84       │
│ Target    master                                         │
│ Branches  add-readme                                     │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 8083d4c Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 07eab0c @ 4c66f0e..8083d4c by alice (you) now │
│ ↑ Revision 2cebe5d @ 602ba44..8083d4c by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

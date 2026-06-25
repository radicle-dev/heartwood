When contributing to another's project, it is common for the contribution to be
of many commits and involve a discussion with the project's maintainer.  This is supported
via Radicle *patches*.

Here we give a brief overview for using patches in our hypothetical car
scenario.  It turns out instructions containing the power requirements were
missing from the project.

```
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
```

Here the instructions are added to the project's `REQUIREMENTS` for 1.21
gigawatts and committed with git.

```
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 602ba44] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

Once the code is ready, we open a patch with our changes.

``` (stderr)
$ git push rad -o no-sync -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch da0b8447cf370a528b6c4a51ff9255eadd726edf opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  da0b844  Define power requirements  bob     (you)  -        602ba44  +0  -0  now             │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
$ rad patch show da0b8447cf370a528b6c4a51ff9255eadd726edf
╭────────────────────────────────────────────────────────╮
│ Title     Define power requirements                    │
│ Patch     da0b8447cf370a528b6c4a51ff9255eadd726edf     │
│ Author    bob (you)                                    │
│ Head      602ba4448210fba26633dc3f9ae3d4d9d20a1e84     │
│ Base      [..                                    ]     │
│ Target    master                                       │
│ Branches  flux-capacitor-power                         │
│ Commits   ahead 1, behind 0                            │
│ Status    open                                         │
│                                                        │
│ See details.                                           │
├────────────────────────────────────────────────────────┤
│ 602ba44 Define power requirements                      │
├────────────────────────────────────────────────────────┤
│ ● Revision da0b844 @ [..   ]..602ba44 by bob (you) now │
╰────────────────────────────────────────────────────────╯
```

We can also confirm that the patch branch is in storage:

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk refs/heads/patches/*
602ba4448210fba26633dc3f9ae3d4d9d20a1e84	refs/heads/patches/da0b8447cf370a528b6c4a51ff9255eadd726edf
```

Wait, let's add a README too! Just for fun.

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 8083d4c] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```
``` (stderr) RAD_SOCKET=/dev/null
$ git push -o patch.message="Add README, just for the fun"
✓ Patch da0b844 updated to revision c6894b5a992a9b5f4c4385d397ee922cc6facc3c
To compare against your previous revision da0b844, run:

   git range-diff 4c66f0e[..] 602ba44[..] 8083d4c[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   602ba44..8083d4c  flux-capacitor-power -> patches/da0b8447cf370a528b6c4a51ff9255eadd726edf
```

And let's leave a quick comment for our team:

```
$ rad patch comment da0b8447cf370a528b6c4a51ff9255eadd726edf --message 'I cannot wait to get back to the 90s!' -q
ec492338e8e111688c732a88ff49ea5536f6b549
✓ Synced with 1 seed(s)
```

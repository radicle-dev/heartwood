Sometimes we are a bit forgetful and miss a detail when creating a
patch. In this case we'll stack two patches by creating one after the
other.

First we add a `REQUIREMENTS` file:

```
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```
``` (stderr)
$ git push rad flux-capacitor-power
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      flux-capacitor-power -> flux-capacitor-power
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 6ff4f09c1b5a81347981f59b02ef43a31a07cdae opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

And then a `README` file:
```
$ git checkout -b add-readme
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[add-readme 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```
``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun" HEAD:refs/patches
✓ Patch 11188b84720e8ab2625be2be64987311fb046573 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Our second patch looks like the following:

```
$ rad patch show 11188b8 -v
╭────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun             │
│ Patch     11188b84720e8ab2625be2be64987311fb046573 │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66 │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ Branches  add-readme                               │
│ Commits   ahead 2, behind 0                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun               │
│ 3e674d1 Define power requirements                  │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) now              │
╰────────────────────────────────────────────────────╯
```

But wait, we meant to stack them and so we don't want to see the
commit `3e674d1` as part of this patch, so we create a new revision
with a new `base`:

```
$ rad patch update 11188b8 -b 3e674d1 -m "Whoops, forgot to set the base" --no-announce
ea90a38b4df0b323149a06b0472ea1cf9a754dca
```

Now, if we show the patch we can see the patch's base has changed and
we have a single commit:

```
$ rad patch show 11188b8 -v
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                              │
│ Patch     11188b84720e8ab2625be2be64987311fb046573                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Base      3e674d1a1df90807e934f9ae5da2591dd6848a33                  │
│ Branches  add-readme                                                │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) now                               │
│ ↑ updated to ea90a38b4df0b323149a06b0472ea1cf9a754dca (27857ec) now │
╰─────────────────────────────────────────────────────────────────────╯
```

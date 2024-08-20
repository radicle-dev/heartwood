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
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      flux-capacitor-power -> flux-capacitor-power
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch c90967c43719b916e0b5a8b5dafe353608f8a08a opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
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
✓ Patch 3e1ca74542ded1f51ca9a744ed6266f23bf2507f opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Our second patch looks like the following:

```
$ rad patch show 3e1ca74542ded1f51ca9a744ed6266f23bf2507f -v
╭────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun             │
│ Patch     3e1ca74542ded1f51ca9a744ed6266f23bf2507f │
│ Author    alice (you)                              │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66 │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 │
│ Branches  add-readme                               │
│ Commits   ahead 2, behind 0                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun               │
│ 3e674d1 Define power requirements                  │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (27857ec) now              │
╰────────────────────────────────────────────────────╯
```

But wait, we meant to stack them and so we don't want to see the
commit `3e674d1` as part of this patch, so we create a new revision
with a new `base`:

```
$ rad patch update 3e1ca74 -b 3e674d1 -m "Whoops, forgot to set the base" --no-announce
852c792f460ca485ce22b6acc41c150d7aeb4642
```

Now, if we show the patch we can see the patch's base has changed and
we have a single commit:

```
$ rad patch show 3e1ca74 -v
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                              │
│ Patch     3e1ca74542ded1f51ca9a744ed6266f23bf2507f                  │
│ Author    alice (you)                                               │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Base      3e674d1a1df90807e934f9ae5da2591dd6848a33                  │
│ Branches  add-readme                                                │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (27857ec) now                               │
│ ↑ updated to 852c792f460ca485ce22b6acc41c150d7aeb4642 (27857ec) now │
╰─────────────────────────────────────────────────────────────────────╯
```

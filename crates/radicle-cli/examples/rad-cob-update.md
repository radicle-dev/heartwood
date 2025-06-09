First off, we set up a patch.

```
$ git checkout -b changes
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[changes 03c02af] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun" HEAD:refs/patches
✓ Patch 89f7afb1511b976482b21f6b2f39aef7f4fb88a2 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ touch LICENSE
$ git add LICENSE
$ git commit -v -m "Define the LICENSE"
[changes 8945f61] Define the LICENSE
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

``` (stderr)
$ git push -f -o patch.message="Add License"
✓ Patch 89f7afb updated to revision 5d78dd5376453e25df5988ec86951c99cb73742c
To compare against your previous revision 89f7afb, run:

   git range-diff f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 03c02af4b12a593d17a06d38fae50a57fc3c339a 8945f6189adf027892c85ac57f7e9341049c2537

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   03c02af..8945f61  changes -> patches/89f7afb1511b976482b21f6b2f39aef7f4fb88a2
```

Let's look at the patch, to see what it looks like before editing it:

```
$ rad patch show 89f7afb
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                              │
│ Patch     89f7afb1511b976482b21f6b2f39aef7f4fb88a2                  │
│ Author    alice (you)                                               │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 5d78dd5376453e25df5988ec86951c99cb73742c (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We can change the title and description of the patch itself by using a
multi-line message (using two `--message` options here):

```
$ rad patch edit 89f7afb --message "Add Metadata" --message "Add README & LICENSE" --no-announce
$ rad patch show 89f7afb
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     89f7afb1511b976482b21f6b2f39aef7f4fb88a2                  │
│ Author    alice (you)                                               │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add README & LICENSE                                                │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 5d78dd5376453e25df5988ec86951c99cb73742c (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We prepare the file `revision-edit.json` which contains one action (thus one line) to be applied.

``` ./revision-edit.jsonl
{"type": "revision.edit", "description": "Add README and LICENSE", "revision": "89f7afb1511b976482b21f6b2f39aef7f4fb88a2"}
```

We now use `rad cob update` to edit the patch another time, rewriting the description.
The action itself is of type `revision.edit` and carries the parameters `revision`,
specifying the revision for which the description should be changed, and `description`,
specifying the new description.

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 89f7afb1511b976482b21f6b2f39aef7f4fb88a2 --message "Edit patch" revision-edit.jsonl
79b816e92735c49b33d93a31890fdf040b36234c
$ rad patch show --verbose 89f7afb
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     89f7afb1511b976482b21f6b2f39aef7f4fb88a2                  │
│ Author    alice (you)                                               │
│ Head      8945f6189adf027892c85ac57f7e9341049c2537                  │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354                  │
│ Branches  changes                                                   │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add README and LICENSE                                              │
├─────────────────────────────────────────────────────────────────────┤
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 5d78dd5376453e25df5988ec86951c99cb73742c (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

Notice that the patch now has the description `Add README and LICENSE`.

We may use `rad cob update` to create a new revision altogether, as well.
Let's create yet another commit, an empty one this time, and do that.

```
$ git commit --allow-empty --message="Dummy commit for a new revision"
[changes f1339dd] Dummy commit for a new revision
```

We prepare the file `revision-create.jsonl` which contains one action.

``` ./revision.jsonl
{"type": "revision", "description": "A new revision", "base": "f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354", "oid": "f1339dd109e538c6b3a7fed3e72403e1b4db08c9"}
```

Attempting to create the new revision right away would fail:

``` (fail)
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 89f7afb1511b976482b21f6b2f39aef7f4fb88a2 --message "Create new revision" revision.jsonl
✗ Error: store: update error: failed to read 'f1339dd109e538c6b3a7fed3e72403e1b4db08c9' from git odb
```

Since we are not using the remote helper `git-remote-rad` here, we need to push
the new commit to storage manually. See `fn patch_open` in `/radicle-remote-helper/src/push.rs`
for more details.

```
$ git push rad HEAD:tmp/heads/f1339dd109e538c6b3a7fed3e72403e1b4db08c9
$ git push rad :tmp/heads/f1339dd109e538c6b3a7fed3e72403e1b4db08c9
```

Now we can invoke `rad cob update`:

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 89f7afb1511b976482b21f6b2f39aef7f4fb88a2 --message "Create new revision" revision.jsonl
2da9c025a1d14d93c4f2cec60a7878afbc5e2a3c
$ rad patch show 89f7afb
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     89f7afb1511b976482b21f6b2f39aef7f4fb88a2                  │
│ Author    alice (you)                                               │
│ Head      f1339dd109e538c6b3a7fed3e72403e1b4db08c9                  │
│ Branches  changes                                                   │
│ Commits   ahead 3, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add README and LICENSE                                              │
├─────────────────────────────────────────────────────────────────────┤
│ f1339dd Dummy commit for a new revision                             │
│ 8945f61 Define the LICENSE                                          │
│ 03c02af Add README, just for the fun                                │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (03c02af) now                               │
│ ↑ updated to 5d78dd5376453e25df5988ec86951c99cb73742c (8945f61) now │
│ ↑ updated to 2da9c025a1d14d93c4f2cec60a7878afbc5e2a3c (f1339dd) now │
╰─────────────────────────────────────────────────────────────────────╯
```
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
✓ Patch f699e2299e9ee734758626924df7e15fd9a68553 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
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
✓ Patch f699e22 updated to revision 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f
To compare against your previous revision f699e22, run:

   git range-diff f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 03c02af4b12a593d17a06d38fae50a57fc3c339a 8945f6189adf027892c85ac57f7e9341049c2537

To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   03c02af..8945f61  changes -> patches/f699e2299e9ee734758626924df7e15fd9a68553
```

Let's look at the patch, to see what it looks like before editing it:

```
$ rad patch show f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
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
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We can change the title and description of the patch itself by using a
multi-line message (using two `--message` options here):

```
$ rad patch edit f699e22 --message "Add Metadata" --message "Add README & LICENSE" --no-announce
$ rad patch show f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
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
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
╰─────────────────────────────────────────────────────────────────────╯
```

We prepare the file `revision-edit.json` which contains one action (thus one line) to be applied.

``` ./revision-edit.jsonl
{"type": "revision.edit", "description": "Add README and LICENSE", "revision": "f699e2299e9ee734758626924df7e15fd9a68553"}
```

We now use `rad cob update` to edit the patch another time, rewriting the description.
The action itself is of type `revision.edit` and carries the parameters `revision`,
specifying the revision for which the description should be changed, and `description`,
specifying the new description.

```
$ rad cob update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch --object f699e2299e9ee734758626924df7e15fd9a68553 --message "Edit patch" revision-edit.jsonl
8f2a1a4c55f976288574e463dff1e85981e7a427
$ rad patch show --verbose f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
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
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
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
$ rad cob update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch --object f699e2299e9ee734758626924df7e15fd9a68553 --message "Create new revision" revision.jsonl
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
$ rad cob update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch --object f699e2299e9ee734758626924df7e15fd9a68553 --message "Create new revision" revision.jsonl
c60de85722655bfd01867c180307a9064c7acabd
$ rad patch show f699e22
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                              │
│ Patch     f699e2299e9ee734758626924df7e15fd9a68553                  │
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
│ ↑ updated to 13b5240e3f7ecb60fea4f66eb1a09fa3ffc1de7f (8945f61) now │
│ ↑ updated to c60de85722655bfd01867c180307a9064c7acabd (f1339dd) now │
╰─────────────────────────────────────────────────────────────────────╯
```

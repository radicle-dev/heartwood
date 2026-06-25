First off, we set up a patch.

```
$ git checkout -b changes
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[changes ad12b3e] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun" HEAD:refs/patches
✓ Patch 564037be20294ec4288f980e8defd394c4572c9a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ touch LICENSE
$ git add LICENSE
$ git commit -v -m "Define the LICENSE"
[changes 098f4c5] Define the LICENSE
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

``` (stderr)
$ git push -f -o patch.message="Add License"
✓ Patch 564037b updated to revision b650ef8d9eb39de4888f2c1a4cd3ee2a3e72f38d
To compare against your previous revision 564037b, run:

   git range-diff 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927 ad12b3e2ddba8485eff6b876de7ada7dc7174778 098f4c53591b72ec2e69a7b33f83d04aa92253e5

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   ad12b3e..098f4c5  changes -> patches/564037be20294ec4288f980e8defd394c4572c9a
```

Let's look at the patch, to see what it looks like before editing it:

```
$ rad patch show 564037b
╭──────────────────────────────────────────────────────────╮
│ Title     Add README, just for the fun                   │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a       │
│ Author    alice (you)                                    │
│ Head      098f4c53591b72ec2e69a7b33f83d04aa92253e5       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  changes                                        │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 098f4c5 Define the LICENSE                               │
│ ad12b3e Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 564037b @ 4c66f0e..ad12b3e by alice (you) now │
│ ↑ Revision b650ef8 @ 4c66f0e..098f4c5 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

We can change the title and description of the patch itself by using a
multi-line message (using two `--message` options here):

```
$ rad patch edit 564037b --message "Add Metadata" --message "Add README & LICENSE" --no-announce
$ rad patch show 564037b
╭──────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                   │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a       │
│ Author    alice (you)                                    │
│ Head      098f4c53591b72ec2e69a7b33f83d04aa92253e5       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  changes                                        │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add README & LICENSE                                     │
├──────────────────────────────────────────────────────────┤
│ 098f4c5 Define the LICENSE                               │
│ ad12b3e Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 564037b @ 4c66f0e..ad12b3e by alice (you) now │
│ ↑ Revision b650ef8 @ 4c66f0e..098f4c5 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

We prepare the file `revision-edit.json` which contains one action (thus one line) to be applied.

``` ./revision-edit.jsonl
{"type": "revision.edit", "description": "Add README and LICENSE", "revision": "564037be20294ec4288f980e8defd394c4572c9a"}
```

We now use `rad cob update` to edit the patch another time, rewriting the description.
The action itself is of type `revision.edit` and carries the parameters `revision`,
specifying the revision for which the description should be changed, and `description`,
specifying the new description.

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 564037be20294ec4288f980e8defd394c4572c9a --message "Edit patch" revision-edit.jsonl
41ae12b703bc616157d3be3f264fad4bc481718e
$ rad patch show --verbose 564037b
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                                                                                                                               │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      098f4c53591b72ec2e69a7b33f83d04aa92253e5                                                                                                                   │
│ Base      4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927                                                                                                                   │
│ Target    refs/heads/master                                                                                                                                          │
│ Branches  changes                                                                                                                                                    │
│ Commits   ahead 2, behind 0                                                                                                                                          │
│ Status    open                                                                                                                                                       │
│                                                                                                                                                                      │
│ Add README and LICENSE                                                                                                                                               │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 098f4c5 Define the LICENSE                                                                                                                                           │
│ ad12b3e Add README, just for the fun                                                                                                                                 │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 564037be20294ec4288f980e8defd394c4572c9a with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..ad12b3e2ddba8485eff6b876de7ada7dc7174778 by alice (you) now │
│ ↑ Revision b650ef8d9eb39de4888f2c1a4cd3ee2a3e72f38d with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..098f4c53591b72ec2e69a7b33f83d04aa92253e5 by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Notice that the patch now has the description `Add README and LICENSE`.

We may use `rad cob update` to create a new revision altogether, as well.
Let's create yet another commit, an empty one this time, and do that.

```
$ git commit --allow-empty --message="Dummy commit for a new revision"
[changes b179e48] Dummy commit for a new revision
```

We prepare the file `revision-create.jsonl` which contains one action.

``` ./revision.jsonl
{"type": "revision", "description": "A new revision", "base": "4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927", "oid": "b179e48cf51f6f21e8b93175489e26f136eb042e"}
```

Attempting to create the new revision right away would fail:

``` (fail)
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 564037be20294ec4288f980e8defd394c4572c9a --message "Create new revision" revision.jsonl
✗ Error: store: update error: failed to read 'b179e48cf51f6f21e8b93175489e26f136eb042e' from git odb
```

Since we are not using the remote helper `git-remote-rad` here, we need to push
the new commit to storage manually. See `fn patch_open` in `/radicle-remote-helper/src/push.rs`
for more details.

```
$ git push rad HEAD:tmp/heads/b179e48cf51f6f21e8b93175489e26f136eb042e
$ git push rad :tmp/heads/b179e48cf51f6f21e8b93175489e26f136eb042e
```

Now we can invoke `rad cob update`:

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 564037be20294ec4288f980e8defd394c4572c9a --message "Create new revision" revision.jsonl
58104a0dc046f566779b02998517e5fbb69307f9
$ rad patch show 564037b
╭──────────────────────────────────────────────────────────╮
│ Title     Add Metadata                                   │
│ Patch     564037be20294ec4288f980e8defd394c4572c9a       │
│ Author    alice (you)                                    │
│ Head      b179e48cf51f6f21e8b93175489e26f136eb042e       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  changes                                        │
│ Commits   ahead 3, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add README and LICENSE                                   │
├──────────────────────────────────────────────────────────┤
│ b179e48 Dummy commit for a new revision                  │
│ 098f4c5 Define the LICENSE                               │
│ ad12b3e Add README, just for the fun                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 564037b @ 4c66f0e..ad12b3e by alice (you) now │
│ ↑ Revision b650ef8 @ 4c66f0e..098f4c5 by alice (you) now │
│ ↑ Revision 58104a0 @ 4c66f0e..b179e48 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

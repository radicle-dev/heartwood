# Changing the target branch of a patch

Open a patch against a branch that has no canonical reference rule. The push
warns, but still opens the patch: whether a target resolves is not settled when
the patch is opened.

``` (stderr)
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
```

```
$ git commit --allow-empty -m "Add new feature"
[feature/1 [..]] Add new feature
```

``` (stderr)
$ git push -o patch.message="Add new feature" -o patch.target="refs/heads/accepted" rad HEAD:refs/patches
warn: head of the target branch `refs/heads/accepted` could not be resolved because there is no rule that matches it
✓ Patch 5e6384c03a8b964cf5c1d934bf5bdb1cd9169dc2 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Retargeting to the same branch is a no-op (should not supply a warning):

```
$ rad patch edit 5e6384c --target refs/heads/accepted --message "Add new feature" --no-announce
```

`rad patch edit --target` moves the patch to the default branch:

```
$ rad patch edit 5e6384c --target delegates --no-announce
$ rad patch show 5e6384c
╭──────────────────────────────────────────────────────────╮
│ Title     Add new feature                                │
│ Patch     5e6384c03a8b964cf5c1d934bf5bdb1cd9169dc2       │
│ Author    alice (you)                                    │
│ Head      2f953624221d06c4c1e7a97637c758509a8558dd       │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 2f95362 Add new feature                                  │
├──────────────────────────────────────────────────────────┤
│ ● Revision 5e6384c @ f2de534..2f95362 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

Retargeting to a branch that has a rule but no commits warns too, and is still
allowed:

```
$ rad id update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": "delegates" } }' -q
[..]
```

```
$ rad patch edit 5e6384c --target refs/heads/accepted --message "Add new feature" --no-announce
! Warning: head of the target branch `refs/heads/accepted` could not be resolved because there is no quorum
```

`--target` is rejected with `--revision`, which edits a revision rather than the
patch:

``` (fail)
$ rad patch edit 5e6384c --revision 5e6384c --target delegates --message "x" --no-announce
✗ Error: `--target` cannot be used with `--revision`
```

Editing only the title leaves the target unchanged:

```
$ rad patch edit 5e6384c --message "Better title" --no-announce
$ rad patch show 5e6384c
╭──────────────────────────────────────────────────────────╮
│ Title     Better title                                   │
│ Patch     5e6384c03a8b964cf5c1d934bf5bdb1cd9169dc2       │
│ Author    alice (you)                                    │
│ Head      2f953624221d06c4c1e7a97637c758509a8558dd       │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354       │
│ Target    accepted                                       │
│ Branches  feature/1                                      │
│ Commits   unknown (target branch unresolvable)           │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 2f95362 Add new feature                                  │
├──────────────────────────────────────────────────────────┤
│ ● Revision 5e6384c @ f2de534..2f95362 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```


# Patches targeting a canonical reference with no commits

Add a canonical reference rule for `accepted`, but never push the branch, so no
`refs/heads/accepted` can be synthesised in storage.

```
$ rad id update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": "delegates" } }' -q
[..]
```

The rule exists, so `refs/heads/accepted` is a valid target. Open a patch for it:

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
warn: head of the target branch `refs/heads/accepted` could not be resolved because there is no quorum
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ●  74cbe24  Add new feature  alice   (you)  -        2f95362  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

Showing it works, with the `Commits` row degraded:

```
$ rad patch show 74cbe24
╭──────────────────────────────────────────────────────────╮
│ Title     Add new feature                                │
│ Patch     74cbe24acc14abd62cc012997d99ec26be379e20       │
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
│ ● Revision 74cbe24 @ f2de534..2f95362 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

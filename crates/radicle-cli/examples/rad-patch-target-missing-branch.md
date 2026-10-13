# Patches that target a branch with no canonical reference

`rad patch show` resolves the target head from `refs/heads/<target>` in
storage. Storage only has that reference when the target is a canonical
reference of the repository. For any other target, the patch still opens and
lists. `rad patch show` displays an unknown commit count. With `--verbose`, it also
shows why the target head cannot be resolved.

## A target branch that does not exist

``` (stderr)
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
```

```
$ git commit --allow-empty -m "Add new feature"
[feature/1 [..]] Add new feature
```

``` (stderr)
$ git push -o patch.message="Add new feature" -o patch.target="refs/heads/nonexistent" rad HEAD:refs/patches
warn: head of the target branch `refs/heads/nonexistent` could not be resolved because there is no rule that matches it
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

The patch lists:

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ●  6b9cd39  Add new feature  alice   (you)  -        2f95362  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

It also shows, with the `Commits` row degraded:

```
$ rad patch show 6b9cd39
╭──────────────────────────────────────────────────────────╮
│ Title     Add new feature                                │
│ Patch     6b9cd39ef8d7e50ebe07b4644300be146d792595       │
│ Author    alice (you)                                    │
│ Head      2f953624221d06c4c1e7a97637c758509a8558dd       │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354       │
│ Target    nonexistent                                    │
│ Branches  feature/1                                      │
│ Commits   unknown                                        │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 2f95362 Add new feature                                  │
├──────────────────────────────────────────────────────────┤
│ ● Revision 6b9cd39 @ f2de534..2f95362 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

With `--verbose`, the `Commits` row explains why the target cannot be resolved:

```
$ rad patch show 6b9cd39 --verbose
...
│ Commits   unknown (head of the target branch `refs/heads/nonexistent` could not be resolved because there is no rule that matches it) [..]│
...
```

## A target branch that exists, but is not canonical

``` (stderr)
$ git checkout master -q
$ git checkout -b sidechain
Switched to a new branch 'sidechain'
```

```
$ git commit --allow-empty -m "Start sidechain"
[sidechain [..]] Start sidechain
```

``` (stderr)
$ git push rad sidechain
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      sidechain -> sidechain
```

``` (stderr)
$ git checkout -b feature/2
Switched to a new branch 'feature/2'
```

```
$ git commit --allow-empty -m "Add second feature"
[feature/2 [..]] Add second feature
```

``` (stderr)
$ git push -o patch.message="Add second feature" -o patch.target="refs/heads/sidechain" rad HEAD:refs/patches
warn: head of the target branch `refs/heads/sidechain` could not be resolved because there is no rule that matches it
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Both patches list, and the second one degrades the same way:

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title               Author         Reviews  Head     +   -   Updated  Labels │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  6b9cd39  Add new feature     alice   (you)  -        2f95362  +0  -0  now             │
│ ●  fd3ecc5  Add second feature  alice   (you)  -        3071f29  +0  -0  now             │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

```
$ rad patch show fd3ecc5
╭──────────────────────────────────────────────────────────╮
│ Title     Add second feature                             │
│ Patch     fd3ecc5dacf184921a0debbee809908c38be538d       │
│ Author    alice (you)                                    │
│ Head      3071f29a3bd95e2f9381310e560ab38b4ee38473       │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354       │
│ Target    sidechain                                      │
│ Branches  feature/2                                      │
│ Commits   unknown                                        │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 3071f29 Add second feature                               │
│ 64da62b Start sidechain                                  │
├──────────────────────────────────────────────────────────┤
│ ● Revision fd3ecc5 @ f2de534..3071f29 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

## Retargeting with `rad cob update`

The COB layer supports changing the target: `Action::Edit` carries a `target`
field. No `rad patch` subcommand exposes it, but `rad cob update` can append
the action directly.

``` ./retarget.jsonl
{"type": "edit", "title": "Add new feature", "target": "delegates"}
```

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 6b9cd39 --message "Retarget to delegates" retarget.jsonl
[..]
```

The patch now targets the default branch, and shows:

```
$ rad patch show 6b9cd39
╭──────────────────────────────────────────────────────────╮
│ Title     Add new feature                                │
│ Patch     6b9cd39ef8d7e50ebe07b4644300be146d792595       │
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
│ ● Revision 6b9cd39 @ f2de534..2f95362 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

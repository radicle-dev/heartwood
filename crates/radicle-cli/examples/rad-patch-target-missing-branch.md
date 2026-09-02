# Patches that target a branch with no canonical reference

`rad patch show` resolves the target head from `refs/heads/<target>` in
storage. Storage only has that reference when the target is a canonical
reference of the repository. For any other target, the patch opens and lists,
but `rad patch show` fails.

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
│ ●  a75d4ea  Add new feature  alice   (you)  -        2f95362  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

But it does not show:

``` (fail)
$ rad patch show a75d4ea
✗ Error: reference 'refs/heads/nonexistent' not found; class=Reference (4); code=NotFound (-3)
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
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Both patches list, and the second one does not show either:

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title               Author         Reviews  Head     +   -   Updated  Labels │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  a75d4ea  Add new feature     alice   (you)  -        2f95362  +0  -0  now             │
│ ●  fd3ecc5  Add second feature  alice   (you)  -        3071f29  +0  -0  now             │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

``` (fail)
$ rad patch show fd3ecc5
✗ Error: reference 'refs/heads/sidechain' not found; class=Reference (4); code=NotFound (-3)
```

## Retargeting with `rad cob update`

The COB layer supports changing the target: `Action::Edit` carries a `target`
field. No `rad patch` subcommand exposes it, but `rad cob update` can append
the action directly.

``` ./retarget.jsonl
{"type": "edit", "title": "Add new feature", "target": "delegates"}
```

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object a75d4ea --message "Retarget to delegates" retarget.jsonl
[..]
```

The patch now targets the default branch, and shows:

```
$ rad patch show a75d4ea
╭──────────────────────────────────────────────────────────╮
│ Title     Add new feature                                │
│ Patch     a75d4ea4837e03c69b9dd2406766fcc6304478ee       │
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
│ ● Revision a75d4ea @ f2de534..2f95362 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

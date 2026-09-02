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

Showing it still fails, even though the target is a legitimate canonical
reference:

``` (fail)
$ rad patch show 74cbe24
✗ Error: reference 'refs/heads/accepted' not found; class=Reference (4); code=NotFound (-3)
```

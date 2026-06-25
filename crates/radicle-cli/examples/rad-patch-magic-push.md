# Magic Push Reference

First, we update the identity document to add a canonical reference rule for a new `accepted` branch, allowing delegates to merge into it.

```
$ rad id update --title "Add accepted branch" --payload xyz.radicle.crefs rules '{ "refs/heads/accepted": { "threshold": 1, "allow": "delegates" } }' -q
[..]
```

Now, let's create the `accepted` branch and push it to the repository so it becomes a tracked canonical reference:

``` (stderr)
$ git checkout -b accepted
Switched to a new branch 'accepted'
```

```
$ git commit --allow-empty -m "Initialize accepted branch"
[accepted [..]] Initialize accepted branch
```

``` (stderr)
$ git push rad accepted
✓ Canonical reference refs/heads/accepted updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      accepted -> accepted
```

We can then use the magic push reference `refs/for/<branch>` to open a patch targeting a specific branch without needing to use push options.

```
$ git checkout -b feature/1 -q
$ git commit -m "Add new feature" --allow-empty -q
```

Pushing to the magic reference:

``` (stderr)
$ git push rad HEAD:refs/for/accepted
✓ Patch df15d2951e27ad8313808070eec2bf6d077ab5f7 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/for/accepted
```

We can see the patch is open:

```
$ rad patch show df15d29
╭──────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                     │
│ Patch     df15d2951e27ad8313808070eec2bf6d077ab5f7       │
│ Author    alice (you)                                    │
│ Head      cc0cbebab028420127659cac3fb66f9a5e11a056       │
│ Base      [..                                    ]       │
│ Target    accepted                                       │
│ Branches  feature/1                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add new feature                                          │
├──────────────────────────────────────────────────────────┤
│ cc0cbeb Add new feature                                  │
│ a3bf012 Initialize accepted branch                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision df15d29 @ 4c66f0e..cc0cbeb by alice (you) now │
╰──────────────────────────────────────────────────────────╯
$ rad patch list --open
╭──────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                       Author         Reviews  Head     +   -   Updated  Labels │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  [..   ]  Initialize accepted branch  alice   (you)  -        [..   ]  +0  -0  now             │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Now we merge the feature into the `accepted` branch:

``` (stderr)
$ git checkout accepted
Switched to branch 'accepted'
```

```
$ git merge feature/1
Updating [..]
Fast-forward
```

``` (stderr)
$ git push rad accepted
✓ Patch [..] merged
✓ Canonical reference refs/heads/accepted updated to target commit [..]
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   [..]..[..]  accepted -> accepted
```

We can now verify that the patch has been successfully marked as merged:

```
$ rad patch show df15d29
╭──────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                     │
│ Patch     df15d2951e27ad8313808070eec2bf6d077ab5f7       │
│ Author    alice (you)                                    │
│ Head      cc0cbebab028420127659cac3fb66f9a5e11a056       │
│ Base      [..                                    ]       │
│ Target    accepted                                       │
│ Branches  accepted, feature/1                            │
│ Commits   up to date                                     │
│ Status    merged                                         │
│                                                          │
│ Add new feature                                          │
├──────────────────────────────────────────────────────────┤
│ cc0cbeb Add new feature                                  │
│ a3bf012 Initialize accepted branch                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision df15d29 @ 4c66f0e..cc0cbeb by alice (you) now │
│   └─ ✓ merged                         by alice (you)     │
╰──────────────────────────────────────────────────────────╯
$ rad patch list --merged
╭──────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                       Author         Reviews  Head     +   -   Updated  Labels │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ✓  [..   ]  Initialize accepted branch  alice   (you)  -        [..   ]  +0  -0  now             │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Alternative, attempting to provide conflicting targets fails:

``` (fail) (stderr)
$ git push -o patch.target=master rad HEAD:refs/for/accepted
error: conflicting merge targets: push option 'refs/heads/master' and magic ref 'refs/heads/accepted' specified
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

However, if the push option and the magic ref match, it succeeds:

```
$ git checkout -b feature/2 -q
$ git commit -m "Add another feature" --allow-empty -q
```

``` (stderr)
$ git push -o patch.target=accepted rad HEAD:refs/for/accepted
✓ Patch 9a3157b91496477722c9a015afcc9ba4cc662180 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/for/accepted
```
```
$ rad patch show 9a3157b
╭──────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                     │
│ Patch     9a3157b91496477722c9a015afcc9ba4cc662180       │
│ Author    alice (you)                                    │
│ Head      070307970291c9f399e5e2658436427eda97b997       │
│ Base      4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927       │
│ Target    accepted                                       │
│ Branches  feature/2                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add new feature                                          │
│                                                          │
│                                                          │
│ Add another feature                                      │
├──────────────────────────────────────────────────────────┤
│ 0703079 Add another feature                              │
│ cc0cbeb Add new feature                                  │
│ a3bf012 Initialize accepted branch                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision 9a3157b @ 4c66f0e..0703079 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

Finally, we can also use a fully qualified branch name in the magic reference:

```
$ git checkout -b feature/3 -q
$ git commit -m "Add a third feature" --allow-empty -q
```
``` (stderr)
$ git push rad HEAD:refs/for/refs/heads/accepted
✓ Patch 3cc46286ac6800224971bdb2f763ad371541e7cb opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/for/refs/heads/accepted
```

This will create a third feature branch and verify that pushing to `refs/for/refs/heads/accepted` successfully opens a patch.

```
$ rad patch show 3cc4628 -v
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                                                                                                                                 │
│ Patch     3cc46286ac6800224971bdb2f763ad371541e7cb                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      d5672c6f4b73aaabb634aa546e523b00b2777786                                                                                                                   │
│ Base      4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927                                                                                                                   │
│ Target    refs/heads/accepted                                                                                                                                        │
│ Branches  feature/3                                                                                                                                                  │
│ Commits   ahead 2, behind 0                                                                                                                                          │
│ Status    open                                                                                                                                                       │
│                                                                                                                                                                      │
│ Add new feature                                                                                                                                                      │
│                                                                                                                                                                      │
│                                                                                                                                                                      │
│ Add another feature                                                                                                                                                  │
│                                                                                                                                                                      │
│                                                                                                                                                                      │
│ Add a third feature                                                                                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ d5672c6 Add a third feature                                                                                                                                          │
│ 0703079 Add another feature                                                                                                                                          │
│ cc0cbeb Add new feature                                                                                                                                              │
│ a3bf012 Initialize accepted branch                                                                                                                                   │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 3cc46286ac6800224971bdb2f763ad371541e7cb with range 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927..d5672c6f4b73aaabb634aa546e523b00b2777786 by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

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
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/for/accepted
```

We can see the patch is open:

```
$ rad patch show 01dad54
╭──────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                     │
│ Patch     01dad54873ada4efa61541e7d90702266d5ced89       │
│ Author    alice (you)                                    │
│ Head      46fd342edc149468ec08b0b25291083aa05d4449       │
│ Base      [..                                    ]       │
│ Branches  feature/1                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add new feature                                          │
├──────────────────────────────────────────────────────────┤
│ 46fd342 Add new feature                                  │
│ f9a3b89 Initialize accepted branch                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision 01dad54 @ f2de534..46fd342 by alice (you) now │
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
$ rad patch show 01dad54
╭──────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                     │
│ Patch     01dad54873ada4efa61541e7d90702266d5ced89       │
│ Author    alice (you)                                    │
│ Head      46fd342edc149468ec08b0b25291083aa05d4449       │
│ Base      [..                                    ]       │
│ Branches  accepted, feature/1                            │
│ Commits   up to date                                     │
│ Status    merged                                         │
│                                                          │
│ Add new feature                                          │
├──────────────────────────────────────────────────────────┤
│ 46fd342 Add new feature                                  │
│ f9a3b89 Initialize accepted branch                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision 01dad54 @ f2de534..46fd342 by alice (you) now │
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
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/for/accepted
```
```
$ rad patch show 965855f
╭──────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                     │
│ Patch     965855f69bc5a86f7e1787ed80cd065013b6504c       │
│ Author    alice (you)                                    │
│ Head      7e6814e99fe7084a213faeb69ca1eacc26935e3c       │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354       │
│ Branches  feature/2                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add new feature                                          │
│                                                          │
│                                                          │
│ Add another feature                                      │
├──────────────────────────────────────────────────────────┤
│ 7e6814e Add another feature                              │
│ 46fd342 Add new feature                                  │
│ f9a3b89 Initialize accepted branch                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision 965855f @ f2de534..7e6814e by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

Finally, we can also use a fully qualified branch name in the magic reference:

```
$ git checkout -b feature/3 -q
$ git commit -m "Add a third feature" --allow-empty -q
```
``` (stderr)
$ git push rad HEAD:refs/for/refs/heads/accepted
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/for/refs/heads/accepted
```

This will create a third feature branch and verify that pushing to `refs/for/refs/heads/accepted` successfully opens a patch.

```
$ rad patch show c846bb5 -v
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Initialize accepted branch                                                                                                                                 │
│ Patch     c846bb5f8298802e8de589174549d78c1d9aa00f                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      99c1d5a04c17ba854f1f4d59985c980c0faf16e7                                                                                                                   │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354                                                                                                                   │
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
│ 99c1d5a Add a third feature                                                                                                                                          │
│ 7e6814e Add another feature                                                                                                                                          │
│ 46fd342 Add new feature                                                                                                                                              │
│ f9a3b89 Initialize accepted branch                                                                                                                                   │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision c846bb5f8298802e8de589174549d78c1d9aa00f with range f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354..99c1d5a04c17ba854f1f4d59985c980c0faf16e7 by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

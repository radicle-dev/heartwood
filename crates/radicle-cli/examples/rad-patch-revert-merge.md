Let's create a patch, merge it and then revert it.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
$ git checkout master
Switched to branch 'master'
$ git merge feature/1
$ git push rad master
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be merged
✓ Canonical reference refs/heads/master updated to target commit f708282adae8d6f31eda23c8ecf3120eb99a499b
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..f708282  master -> master
```

First we see the patch as merged.

```
$ rad patch show 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
╭──────────────────────────────────────────────────────────╮
│ Title     First change                                   │
│ Patch     91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be       │
│ Author    alice (you)                                    │
│ Head      f708282adae8d6f31eda23c8ecf3120eb99a499b       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/1, master                              │
│ Commits   up to date                                     │
│ Status    merged                                         │
├──────────────────────────────────────────────────────────┤
│ f708282 First change                                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 91f94ec @ [..   ]..f708282 by alice (you) now │
│   └─ ✓ merged                         by alice (you)     │
╰──────────────────────────────────────────────────────────╯
```

Now let's revert the patch by pushing a new `master` that doesn't include
the commit.

```
$ git reset --hard HEAD^
HEAD is now at 4c66f0e Second commit
```

When pushing, notice that we're told our patch is reverted.

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad master --force
! Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be reverted at revision 91f94ec
✓ Canonical reference refs/heads/master updated to target commit 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + f708282...4c66f0e master -> master (forced update)
```

The patch shows up as open again.

```
$ rad patch show 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be
╭──────────────────────────────────────────────────────────╮
│ Title     First change                                   │
│ Patch     91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be       │
│ Author    alice (you)                                    │
│ Head      f708282adae8d6f31eda23c8ecf3120eb99a499b       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ f708282 First change                                     │
├──────────────────────────────────────────────────────────┤
│ ● Revision 91f94ec @ [..   ]..f708282 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

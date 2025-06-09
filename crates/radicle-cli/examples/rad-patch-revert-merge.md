Let's create a patch, merge it and then revert it.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 696ec5508494692899337afe6713fe1796d0315c opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
$ git checkout master
Switched to branch 'master'
$ git merge feature/1
$ git push rad master
✓ Patch 696ec5508494692899337afe6713fe1796d0315c merged
✓ Canonical head updated to 20aa5dde6210796c3a2f04079b42316a31d02689
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..20aa5dd  master -> master
```

First we see the patch as merged.

```
$ rad patch show 696ec5508494692899337afe6713fe1796d0315c
╭────────────────────────────────────────────────────────────────╮
│ Title     First change                                         │
│ Patch     696ec5508494692899337afe6713fe1796d0315c             │
│ Author    alice (you)                                          │
│ Head      20aa5dde6210796c3a2f04079b42316a31d02689             │
│ Branches  feature/1, master                                    │
│ Commits   up to date                                           │
│ Status    merged                                               │
├────────────────────────────────────────────────────────────────┤
│ 20aa5dd First change                                           │
├────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (20aa5dd) now                          │
│   └─ ✓ merged by alice (you) at revision 696ec55 (20aa5dd) now │
╰────────────────────────────────────────────────────────────────╯
```

Now let's revert the patch by pushing a new `master` that doesn't include
the commit.

```
$ git reset --hard HEAD^
HEAD is now at f2de534 Second commit
```

When pushing, notice that we're told our patch is reverted.

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad master --force
! Patch 696ec5508494692899337afe6713fe1796d0315c reverted at revision 696ec55
✓ Canonical head updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 20aa5dd...f2de534 master -> master (forced update)
```

The patch shows up as open again.

```
$ rad patch show 696ec5508494692899337afe6713fe1796d0315c
╭────────────────────────────────────────────────────╮
│ Title     First change                             │
│ Patch     696ec5508494692899337afe6713fe1796d0315c │
│ Author    alice (you)                              │
│ Head      20aa5dde6210796c3a2f04079b42316a31d02689 │
│ Branches  feature/1                                │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 20aa5dd First change                               │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (20aa5dd) now              │
╰────────────────────────────────────────────────────╯
```

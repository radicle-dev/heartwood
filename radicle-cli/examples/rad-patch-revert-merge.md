Let's create a patch, merge it and then revert it.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
$ git checkout master
Switched to branch 'master'
$ git merge feature/1
$ git push rad master
✓ Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 merged
✓ Canonical head for refs/heads/master updated to 20aa5dde6210796c3a2f04079b42316a31d02689
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..20aa5dd  master -> master
```

First we see the patch as merged.

```
$ rad patch show 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084
╭────────────────────────────────────────────────────────────────╮
│ Title     First change                                         │
│ Patch     09a3de4ac2c4d012c4a9c84c0cb306a066a0b084             │
│ Author    alice (you)                                          │
│ Head      20aa5dde6210796c3a2f04079b42316a31d02689             │
│ Branches  feature/1, master                                    │
│ Commits   up to date                                           │
│ Status    merged                                               │
├────────────────────────────────────────────────────────────────┤
│ 20aa5dd First change                                           │
├────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (20aa5dd) now                          │
│   └─ ✓ merged by alice (you) at revision 09a3de4 (20aa5dd) now │
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
! Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 reverted at revision 09a3de4
✓ Canonical head for refs/heads/master updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 20aa5dd...f2de534 master -> master (forced update)
```

The patch shows up as open again.

```
$ rad patch show 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084
╭────────────────────────────────────────────────────╮
│ Title     First change                             │
│ Patch     09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 │
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

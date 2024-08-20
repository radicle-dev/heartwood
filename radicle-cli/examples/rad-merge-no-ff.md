Let's test that merge commits are handled properly in the context of patches.
First, let's create a patch.
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then let's update the master branch so that merging the patch would create a merge commit.
```
$ git checkout master -q
$ git commit --amend --allow-empty -q -m "Concurrent change"
$ git rev-parse HEAD
f65977beef04fcc5cd5395feed7ff4c37cd90a2f
```

Now let's merge the patch, creating a merge commit. We can see that one of the
parents is the patch head.
```
$ git merge feature/1 -q --no-ff
$ git show --format=raw HEAD
commit 737a10cfa29111afeb0d43cf3545cee386b939ec
tree b4eecafa9be2f2006ce1b709d6857b07069b4608
parent f65977beef04fcc5cd5395feed7ff4c37cd90a2f
parent 20aa5dde6210796c3a2f04079b42316a31d02689
author radicle <radicle@localhost> 1671125284 +0000
committer radicle <radicle@localhost> 1671125284 +0000

    Merge branch 'feature/1'

```

Finally, we push master and expect the patch to be merged.
``` (stderr) RAD_SOCKET=/dev/null
$ git push rad master
✓ Patch 09a3de4ac2c4d012c4a9c84c0cb306a066a0b084 merged
✓ Canonical head for refs/heads/master updated to 737a10cfa29111afeb0d43cf3545cee386b939ec
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..737a10c  master -> master
```

Let's test that merge commits are handled properly in the context of patches.
First, let's create a patch.
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then let's update the master branch so that merging the patch would create a merge commit.
```
$ git checkout master -q
$ git commit --amend --allow-empty -q -m "Concurrent change"
$ git rev-parse HEAD
1a0396eeb611d028bc7f2e82b02ff97eceed7cb1
```

Now let's merge the patch, creating a merge commit. We can see that one of the
parents is the patch head.
```
$ git merge feature/1 -q --no-ff
$ git show --format=raw HEAD
commit da97078798d6163e9a4678aecd75a6bd9302b1a3
tree b4eecafa9be2f2006ce1b709d6857b07069b4608
parent 1a0396eeb611d028bc7f2e82b02ff97eceed7cb1
parent f708282adae8d6f31eda23c8ecf3120eb99a499b
author radicle <radicle@localhost> 1671125284 +0000
committer radicle <radicle@localhost> 1671125284 +0000

    Merge branch 'feature/1'

```

Finally, we push master and expect the patch to be merged.
``` (stderr) RAD_SOCKET=/dev/null
$ git push rad master
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be merged
✓ Canonical reference refs/heads/master updated to target commit da97078798d6163e9a4678aecd75a6bd9302b1a3
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..da97078  master -> master
```

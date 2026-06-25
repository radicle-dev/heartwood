Let's start by creating a patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then let's update the code and merge the updated code without updating the patch:

``` (stderr) RAD_SOCKET=/dev/null
$ git commit --amend --allow-empty -q -m "Amended change"
$ git checkout master -q
$ git merge feature/1 -q
$ git push rad master
✓ Canonical reference refs/heads/master updated to target commit 01f11940ef700c8c3e2a6c8f20894c4ef3719dd1
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..01f1194  master -> master
```

As we can see, no patch is merged. Now if we go back to our patch and try to
update it, we expect it to be updated and merged:

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout feature/1 -q
$ git push -f
✓ Patch 91f94ec updated to revision [...]
✓ Patch 91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + f708282...01f1194 feature/1 -> patches/91f94ec3b0b1a2fbf6d2f0a18f8c63e9713ac7be (forced update)
```

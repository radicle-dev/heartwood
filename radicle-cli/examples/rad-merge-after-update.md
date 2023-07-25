Let's start by creating a patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 0ec956c94256fa101db4c32956ce195a1aa0edf2 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then let's update the code and merge the updated code without updating the patch:

``` (stderr) RAD_SOCKET=/dev/null
$ git commit --amend --allow-empty -q -m "Amended change"
$ git checkout master -q
$ git merge feature/1 -q
$ git push rad master
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..954bcdb  master -> master
```

As we can see, no patch is merged. Now if we go back to our patch and try to
update it, we expect it to be updated and merged:

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout feature/1 -q
$ git push -f
✓ Patch 0ec956c updated to 8175b00f4d75059976930cfcb75ef08454c87055
✓ Patch 0ec956c94256fa101db4c32956ce195a1aa0edf2 merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 20aa5dd...954bcdb feature/1 -> patches/0ec956c94256fa101db4c32956ce195a1aa0edf2 (forced update)
```

Let's start by creating a patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch 696ec5508494692899337afe6713fe1796d0315c opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then let's update the code and merge the updated code without updating the patch:

``` (stderr) RAD_SOCKET=/dev/null
$ git commit --amend --allow-empty -q -m "Amended change"
$ git checkout master -q
$ git merge feature/1 -q
$ git push rad master
✓ Canonical head updated to 954bcdb5008447ce294a61a21d7eb87afbe7f4a6
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..954bcdb  master -> master
```

As we can see, no patch is merged. Now if we go back to our patch and try to
update it, we expect it to be updated and merged:

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout feature/1 -q
$ git push -f
✓ Patch 696ec55 updated to revision [...]
✓ Patch 696ec5508494692899337afe6713fe1796d0315c merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 20aa5dd...954bcdb feature/1 -> patches/696ec5508494692899337afe6713fe1796d0315c (forced update)
```

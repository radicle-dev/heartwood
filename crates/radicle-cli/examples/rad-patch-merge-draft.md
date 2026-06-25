Let's start by creating a draft patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push -o patch.draft rad HEAD:refs/patches
✓ Patch 32229d20c3477c39c03ded4ad9a15d3d3347bbaf drafted
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout master -q
$ git merge feature/1
$ git push rad master
✓ Patch 32229d20c3477c39c03ded4ad9a15d3d3347bbaf merged
✓ Canonical reference refs/heads/master updated to target commit f708282adae8d6f31eda23c8ecf3120eb99a499b
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..f708282  master -> master
```

Let's start by creating a draft patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push -o patch.draft rad HEAD:refs/patches
✓ Patch d3b1d28d4c566e81ea3b8dcd2a23faef3e2471a5 drafted
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout master -q
$ git merge feature/1
$ git push rad master
✓ Patch d3b1d28d4c566e81ea3b8dcd2a23faef3e2471a5 merged
✓ Canonical head updated to 20aa5dde6210796c3a2f04079b42316a31d02689
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..20aa5dd  master -> master
```

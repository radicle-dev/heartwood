Let's start by creating a draft patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push -o patch.draft rad HEAD:refs/patches
✓ Patch cf29ac6b10141058be66b94a92a81c703b972751 drafted
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout master -q
$ git merge feature/1
$ git push rad master
✓ Patch cf29ac6b10141058be66b94a92a81c703b972751 merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..20aa5dd  master -> master
```

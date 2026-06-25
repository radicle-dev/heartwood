Finally, we can also delete branches with `git push`:

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/*
9d6273e3d5393db7c029f03c7b61d3923bb7366b	refs/heads/alice/1
d59fd2286b81e88537e467216084930de7b9e710	refs/heads/alice/2
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	refs/heads/master
```

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad :alice/1
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 - [deleted]         alice/1
```

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/*
d59fd2286b81e88537e467216084930de7b9e710	refs/heads/alice/2
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	refs/heads/master
```

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout alice/2
Switched to a new branch 'alice/2'
$ git push rad HEAD:refs/patches
✓ Patch ab0d5faabdf29b0aac5846a26ddff6ee63da97f0 opened
To [..]
 * [new reference]   HEAD -> refs/patches
```

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad alice/2 -d
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 - [deleted]         alice/2
```

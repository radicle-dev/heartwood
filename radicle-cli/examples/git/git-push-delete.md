Finally, we can also delete branches with `git push`:

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/*
145e1e69bef3ad93d14946ea212249c2fa9b9828	refs/heads/alice/1
ddcc1f164eacfd7dba41da9bff3261da3ee79fd3	refs/heads/alice/2
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad :alice/1
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 - [deleted]         alice/1
```

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/*
ddcc1f164eacfd7dba41da9bff3261da3ee79fd3	refs/heads/alice/2
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout alice/2
Switched to a new branch 'alice/2'
$ git push rad HEAD:refs/patches
✓ Patch bb9b0d5b8de8d5e2a4cba45f02bd35b3e2678fbe opened
To [..]
 * [new reference]   HEAD -> refs/patches
```

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad alice/2 -d
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 - [deleted]         alice/2
```

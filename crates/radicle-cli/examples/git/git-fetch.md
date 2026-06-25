```
$ cd heartwood
$ git branch -r
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
  rad/master
```

```
$ git ls-remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/heads/*'
9d6273e3d5393db7c029f03c7b61d3923bb7366b	refs/heads/alice/1
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	refs/heads/master
```

``` (stderr)
$ git fetch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      alice/1    -> alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/alice/1
```

```
$ git branch -r
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/alice/1
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
  rad/master
```

```
$ git rev-parse alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/alice/1
9d6273e3d5393db7c029f03c7b61d3923bb7366b
```

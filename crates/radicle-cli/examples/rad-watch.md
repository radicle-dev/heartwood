The `rad watch` command allows you to watch a reference and return when it
reaches a target commit.

``` ~bob
$ git rev-parse refs/remotes/alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
```

``` ~alice
$ git rev-parse master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
$ git commit --allow-empty -m "Minor update" -q
$ git rev-parse master
2c12e482095ffb0dadda4b3c9a9bbd624d0236a2
$ git push rad master
```

``` ~bob
$ rad watch --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --node z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --ref 'refs/heads/master' --target 2c12e482095ffb0dadda4b3c9a9bbd624d0236a2 --interval 500
```

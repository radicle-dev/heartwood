The `rad watch` command allows you to watch a reference and return when it
reaches a target commit.

``` ~bob
$ git rev-parse refs/remotes/alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

``` ~alice
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
$ git commit --allow-empty -m "Minor update" -q
$ git rev-parse master
e09c4dc1b54443ceea715ea648afecdcfd1dd7d0
$ git push rad master
```

``` ~bob
$ rad watch --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --node z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --ref 'refs/heads/master' --target e09c4dc1b54443ceea715ea648afecdcfd1dd7d0 --interval 500
```

```
$ cd heartwood
$ git branch -r
  rad/master
  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

```
$ git ls-remote z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/heads/*'
145e1e69bef3ad93d14946ea212249c2fa9b9828	refs/heads/alice/1
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

``` (stderr)
$ git fetch z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      alice/1    -> z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/alice/1
```

```
$ git branch -r
  rad/master
  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/alice/1
  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

```
$ git rev-parse z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/alice/1
145e1e69bef3ad93d14946ea212249c2fa9b9828
```

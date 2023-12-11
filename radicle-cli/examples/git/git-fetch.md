```
$ cd heartwood
$ git branch -r
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
  rad/master
```

```
$ git ls-remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/heads/*'
145e1e69bef3ad93d14946ea212249c2fa9b9828	refs/heads/alice/1
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
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
145e1e69bef3ad93d14946ea212249c2fa9b9828
```

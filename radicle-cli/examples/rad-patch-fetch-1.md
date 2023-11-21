This is a simple test to ensure the behavior of our remote helper is correct.

``` ~alice
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
$ git checkout -b alice/1 -q
$ git commit --allow-empty -m "Change #1" -q
$ git rev-parse HEAD
7461703ce0fda972df450d071d1d3702057a6352
$ git push rad HEAD:alice/1
```

``` ~bob
$ git status
On branch master
Your branch is up to date with 'rad/master'.

nothing to commit, working tree clean
$ git fetch --all
Fetching rad
Fetching alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ cat .git/FETCH_HEAD
7461703ce0fda972df450d071d1d3702057a6352	not-for-merge	branch 'alice/1' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	not-for-merge	branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ git merge FETCH_HEAD
Already up to date.
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
$ git rev-parse HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

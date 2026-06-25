This is a simple test to ensure the behavior of our remote helper is correct.

``` ~alice
$ git rev-parse master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
$ git checkout -b alice/1 -q
$ git commit --allow-empty -m "Change #1" -q
$ git rev-parse HEAD
78b013fe3bef40eae76e1c3489d736687a23a0fe
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
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927		branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
78b013fe3bef40eae76e1c3489d736687a23a0fe	not-for-merge	branch 'alice/1' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	not-for-merge	branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ git merge FETCH_HEAD
Already up to date.
$ git rev-parse master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
$ git rev-parse HEAD
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
```

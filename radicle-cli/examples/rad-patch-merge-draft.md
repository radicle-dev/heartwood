Let's start by creating a draft patch.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push -o patch.draft rad HEAD:refs/patches
✓ Patch 304d0a97bf9c4dadd4c732196a0f68dcfb2b6738 drafted
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout master -q
$ git merge feature/1
$ git push rad master
✓ Patch 304d0a97bf9c4dadd4c732196a0f68dcfb2b6738 merged
✓ Canonical head for refs/heads/master updated to 20aa5dde6210796c3a2f04079b42316a31d02689
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..20aa5dd  master -> master
```

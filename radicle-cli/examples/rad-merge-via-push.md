Let's start by creating two patches.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/1 -q
$ git commit --allow-empty -q -m "First change"
$ git push rad HEAD:refs/patches
✓ Patch f4e9dcffb21bee746e0eee965933c7e237aa207a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```
``` (stderr) RAD_SOCKET=/dev/null
$ git checkout -b feature/2 -q
$ git commit --allow-empty -q -m "Second change"
$ git push rad HEAD:refs/patches
✓ Patch dce2ff0b2baf6da67fae5143b828ebfab65d41e4 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then let's merge the changes into `master`.

``` (stderr) RAD_SOCKET=/dev/null
$ git checkout master
Switched to branch 'master'
$ git merge feature/1
$ git merge feature/2
```

When we push to `rad/master`, we automatically merge the patches:

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad master
✓ Patch dce2ff0b2baf6da67fae5143b828ebfab65d41e4 merged
✓ Patch f4e9dcffb21bee746e0eee965933c7e237aa207a merged
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..e9fff34  master -> master
```
```
$ rad patch --merged
╭──────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title          Author                  Head     +   -   Updated      │
├──────────────────────────────────────────────────────────────────────────────────┤
│ ✔  dce2ff0  Second change  z6MknSL…StBU8Vi  (you)  e9fff34  +0  -0  [   ...    ] │
│ ✔  f4e9dcf  First change   z6MknSL…StBU8Vi  (you)  20aa5dd  +0  -0  [   ...    ] │
╰──────────────────────────────────────────────────────────────────────────────────╯
```

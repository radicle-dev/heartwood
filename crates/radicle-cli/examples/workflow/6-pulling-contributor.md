Now that the patch is merged, we can update our master branch to the canonical
master, which includes our patch.

First, we confirm that our master is behind:
```
$ git rev-parse master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
```

Then, we call `rad sync --fetch` to fetch from the maintainer:
```
$ rad sync --fetch
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Now let's checkout `master` and pull the maintainer's changes:
```
$ git checkout master
Your branch is up to date with 'rad/master'.
```
``` (stderr) RAD_SOCKET=/dev/null
$ git pull --all --ff
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
   4c66f0e..6947d0d  master     -> rad/master
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..6947d0d  master     -> alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

Now our master branch is up to date with the maintainer's master:

```
$ git rev-parse master
6947d0dc43b7e5f7902bbe21550f1dfc1e54b205
$ git diff master..alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

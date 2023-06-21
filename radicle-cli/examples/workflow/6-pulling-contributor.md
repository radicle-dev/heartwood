Now that the patch is merged, we can update our master branch to the canonical
master, which includes our patch.

First, we confirm that our master is behind:
```
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

Then, we call `rad sync --fetch` to fetch from the maintainer:
```
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
```

Now let's checkout `master` and pull the maintainer's changes:
```
$ git checkout master
Your branch is up to date with 'rad/master'.
```
``` (stderr)
$ git pull --all --ff
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
   f2de534..f567f69  master     -> rad/master
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..f567f69  master     -> z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

Now our master branch is up to date with the maintainer's master:

```
$ git rev-parse master
f567f695d25b4e8fb63b5f5ad2a584529826e908
$ git diff master..z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

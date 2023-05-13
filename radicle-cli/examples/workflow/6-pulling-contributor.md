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
$ git pull --all --ff
Fetching rad
Fetching z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Updating f2de534..f6484e0
Fast-forward
 README.md       | 0
 REQUIREMENTS.md | 0
 2 files changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
 create mode 100644 REQUIREMENTS.md
```

Now our master branch is up to date with the maintainer's master:

```
$ git rev-parse master
f6484e0f43e48a8983b9b39bf9bd4cd889f1d520
$ git diff master..z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master
```

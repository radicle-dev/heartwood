Ensure that we're able to pull our own branches if they happened to be deleted
in our working copy. This also applies to a situation with multiple working
copies.

```
$ git checkout -b alice/1 -q
$ git commit --allow-empty -m "Changes #1" -q
$ git push rad -o patch.message="Changes" HEAD:refs/patches
```

```
$ git checkout master -q
$ git branch -D alice/1 -q
$ git update-ref -d refs/remotes/rad/alice/1
$ git update-ref -d refs/remotes/rad/patches/5e2dedcc5d515fcbc1cca483d3376609fe889bfb
$ git gc --prune=now
$ git branch -r
  rad/master
```

```
$ git pull
Already up to date.
$ git branch -r
  rad/master
  rad/patches/5e2dedcc5d515fcbc1cca483d3376609fe889bfb
```

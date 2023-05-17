# Using `git push` to open patches

Let's checkout a branch, make a commit and push to the magic ref `refs/patches`.
When we push to this ref, a patch is created from our commits.

``` (stderr)
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ git commit -a -m "Add things" -q --allow-empty
$ git push rad HEAD:refs/patches
✓ Patch 8f0e8ecb47a17e8f3219f33150a4092d645e4875 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can see a patch was created:

```
$ rad patch show 8f0e8ec
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add things                                                                    │
│ Patch     8f0e8ecb47a17e8f3219f33150a4092d645e4875                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      42d894a83c9c356552a57af09ccdbd5587a99045                                      │
│ Branches  feature/1                                                                     │
│ Commits   ahead 1, behind 0                                                             │
│ Status    open                                                                          │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 42d894a Add things                                                                      │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [    ..    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

If we check our local branch, we can see its upstream is set to track a remote
branch associated with this patch:

```
$ git branch -vv
* feature/1 42d894a [rad/patches/8f0e8ecb47a17e8f3219f33150a4092d645e4875] Add things
  master    f2de534 [rad/master] Second commit
```

Let's check that it's up to date with our local head:

```
$ git status --short --branch
## feature/1...rad/patches/8f0e8ecb47a17e8f3219f33150a4092d645e4875
$ git fetch
$ git push
```

And let's look at our local and remote refs:

```
$ git show-ref
42d894a83c9c356552a57af09ccdbd5587a99045 refs/heads/feature/1
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/heads/master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/remotes/rad/master
42d894a83c9c356552a57af09ccdbd5587a99045 refs/remotes/rad/patches/8f0e8ecb47a17e8f3219f33150a4092d645e4875
```
```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/heads/patches/*'
42d894a83c9c356552a57af09ccdbd5587a99045	refs/heads/patches/8f0e8ecb47a17e8f3219f33150a4092d645e4875
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/cobs/*'
8f0e8ecb47a17e8f3219f33150a4092d645e4875	refs/cobs/xyz.radicle.patch/8f0e8ecb47a17e8f3219f33150a4092d645e4875
```

We can also create patches by pushing to the `rad/patches` remote. It's a bit
simpler:

``` (stderr)
$ git checkout -b feature/2 -q
$ git commit -a -m "Add more things" -q --allow-empty
$ git push rad/patches
✓ Patch 8678aafff1d1e28430952abf431e60b87e28023c opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We see both branches with upstreams now:

```
$ git branch -vv
  feature/1 42d894a [rad/patches/8f0e8ecb47a17e8f3219f33150a4092d645e4875] Add things
* feature/2 b94a835 [rad/patches/8678aafff1d1e28430952abf431e60b87e28023c] Add more things
  master    f2de534 [rad/master] Second commit
```

And both patches:

```
$ rad patch
╭────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author                  Head     +   -   Updated      │
├────────────────────────────────────────────────────────────────────────────────────┤
│ ●  8678aaf  Add more things  z6MknSL…StBU8Vi  (you)  b94a835  +0  -0  [    ...   ] │
│ ●  8f0e8ec  Add things       z6MknSL…StBU8Vi  (you)  42d894a  +0  -0  [    ...   ] │
╰────────────────────────────────────────────────────────────────────────────────────╯
```

Note that we can't fetch from `rad/patches`:

``` (stderr) (fail)
$ git fetch rad/patches
fatal: couldn't find remote ref refs/patches
```

To update our patch, we simply push commits to the upstream branch:

```
$ git commit -a -m "Improve code" -q --allow-empty
```

``` (stderr)
$ git push
✓ Patch 8678aaf updated to 4b6618a6ccb0b406659364a70a00bb60e4cd7cf0
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   b94a835..662843e  feature/2 -> patches/8678aafff1d1e28430952abf431e60b87e28023c
```

This last `git push` worked without specifying an upstream branch despite the
local branch having a different name than the remote. This is because Radicle
configures repositories upon `rad init` with `push.default = upstream`:

```
$ git config --local --get push.default
upstream
```

This allows for pushing to the remote patch branch without using the full
`<src>:<dst>` syntax.

We can then see that the patch head has moved:

```
$ rad patch show 8678aaf
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add more things                                                               │
│ Patch     8678aafff1d1e28430952abf431e60b87e28023c                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      662843ed81e76efa69d7901fb7bdd775043015d0                                      │
│ Branches  feature/2                                                                     │
│ Commits   ahead 3, behind 0                                                             │
│ Status    open                                                                          │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 662843e Improve code                                                                    │
│ b94a835 Add more things                                                                 │
│ 42d894a Add things                                                                      │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [   ...    ] │
│ ↑ updated to 4b6618a6ccb0b406659364a70a00bb60e4cd7cf0 (662843e) [              ...    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

And we can check that all the refs are properly updated in our repository:

```
$ git rev-parse HEAD
662843ed81e76efa69d7901fb7bdd775043015d0
```

```
$ git status --short --branch
## feature/2...rad/patches/8678aafff1d1e28430952abf431e60b87e28023c
```

```
$ git rev-parse refs/remotes/rad/patches/8678aafff1d1e28430952abf431e60b87e28023c
662843ed81e76efa69d7901fb7bdd775043015d0
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/patches/8678aafff1d1e28430952abf431e60b87e28023c
662843ed81e76efa69d7901fb7bdd775043015d0	refs/heads/patches/8678aafff1d1e28430952abf431e60b87e28023c
```

## Force push

Sometimes, it's necessary to force-push a patch update. For example, if we amended
the commit and want the updated patch to reflect that.

Let's try.

```
$ git commit --amend -m "Amended commit" --allow-empty
[feature/2 3507cd5] Amended commit
 Date: [..]
```

Now let's push to the patch head.

``` (stderr) (fail)
$ git push
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [rejected]        feature/2 -> patches/8678aafff1d1e28430952abf431e60b87e28023c (non-fast-forward)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
hint: Updates were rejected because a pushed branch tip is behind its remote
hint: counterpart. Check out this branch and integrate the remote changes
hint: (e.g. 'git pull ...') before pushing again.
hint: See the 'Note about fast-forwards' in 'git push --help' for details.
```

The push fails because it's not a fast-forward update. To remedy this, we can
use `--force` to force the update.

``` (stderr)
$ git push --force
✓ Patch 8678aaf updated to 983f2d21c9fbe91c21ddfbcd3e3d6cd13db0a944
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 662843e...3507cd5 feature/2 -> patches/8678aafff1d1e28430952abf431e60b87e28023c (forced update)
```

That worked. We can see the new revision if we call `rad patch show`:

```
$ rad patch show 8678aaf
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add more things                                                               │
│ Patch     8678aafff1d1e28430952abf431e60b87e28023c                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      3507cd57811fe5f21f6a0a610a1ac8068b3a5d9f                                      │
│ Branches  feature/2                                                                     │
│ Commits   ahead 3, behind 0                                                             │
│ Status    open                                                                          │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 3507cd5 Amended commit                                                                  │
│ b94a835 Add more things                                                                 │
│ 42d894a Add things                                                                      │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [    ...   ] │
│ ↑ updated to 4b6618a6ccb0b406659364a70a00bb60e4cd7cf0 (662843e) [    ...   ]            │
│ ↑ updated to 983f2d21c9fbe91c21ddfbcd3e3d6cd13db0a944 (3507cd5) [    ...   ]            │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

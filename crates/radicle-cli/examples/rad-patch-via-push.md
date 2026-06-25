# Using `git push` to open patches

Let's checkout a branch, make a commit and push to the magic ref `refs/patches`.
When we push to this ref, a patch is created from our commits.

``` (stderr) RAD_HINT=1
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ git commit -a -m "Add things" -q --allow-empty
$ git push -o patch.message="Add things #1" -o patch.message="See commits for details." rad HEAD:refs/patches
✓ Patch 6c2fe32a2f0659721e51a298250fbfb7b3081a52 opened
hint: to update, run `git push` or `git push rad --force-with-lease HEAD:patches/6c2fe32a2f0659721e51a298250fbfb7b3081a52`
hint: offline push, your node is not running
hint: to sync with the network, run `rad node start`
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can see a patch was created:

```
$ rad patch show 6c2fe32a2f0659721e51a298250fbfb7b3081a52
╭──────────────────────────────────────────────────────────╮
│ Title     Add things #1                                  │
│ Patch     6c2fe32a2f0659721e51a298250fbfb7b3081a52       │
│ Author    alice (you)                                    │
│ Head      a070abbf4dcfa71b66251fa7bca1119bed92120c       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ See commits for details.                                 │
├──────────────────────────────────────────────────────────┤
│ a070abb Add things                                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision 6c2fe32 @ [..   ]..a070abb by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

If we check our local branch, we can see its upstream is set to track a remote
branch associated with this patch:

```
$ git branch -vv
* feature/1 a070abb [rad/patches/6c2fe32a2f0659721e51a298250fbfb7b3081a52] Add things
  master    4c66f0e [rad/master] Second commit
```

Let's check that it's up to date with our local head:

```
$ git status --short --branch
## feature/1...rad/patches/6c2fe32a2f0659721e51a298250fbfb7b3081a52
$ git fetch
$ git push
```

And let's look at our local and remote refs:

```
$ git show-ref
a070abbf4dcfa71b66251fa7bca1119bed92120c refs/heads/feature/1
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927 refs/heads/master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927 refs/remotes/rad/master
a070abbf4dcfa71b66251fa7bca1119bed92120c refs/remotes/rad/patches/6c2fe32a2f0659721e51a298250fbfb7b3081a52
```
```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji 'refs/heads/patches/*'
a070abbf4dcfa71b66251fa7bca1119bed92120c	refs/heads/patches/6c2fe32a2f0659721e51a298250fbfb7b3081a52
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/cobs/*'
0656c217f917c3e06234771e9ecae53aba5e173e	refs/cobs/xyz.radicle.id/0656c217f917c3e06234771e9ecae53aba5e173e
6c2fe32a2f0659721e51a298250fbfb7b3081a52	refs/cobs/xyz.radicle.patch/6c2fe32a2f0659721e51a298250fbfb7b3081a52
```

We can create another patch:

``` (stderr)
$ git checkout -b feature/2 -q master
$ git commit -a -m "Add more things" -q --allow-empty
$ git push rad HEAD:refs/patches
✓ Patch 590b60dcf7618bd968fc7572131a732b0863bd10 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We see both branches with upstreams now:

```
$ git branch -vv
  feature/1 a070abb [rad/patches/6c2fe32a2f0659721e51a298250fbfb7b3081a52] Add things
* feature/2 f63e783 [rad/patches/590b60dcf7618bd968fc7572131a732b0863bd10] Add more things
  master    4c66f0e [rad/master] Second commit
```

And both patches:

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated  Labels │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ●  590b60d  Add more things  alice   (you)  -        f63e783  +0  -0  now             │
│ ●  6c2fe32  Add things #1    alice   (you)  -        a070abb  +0  -0  now             │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

To update our patch, we simply push commits to the upstream branch:

```
$ git commit -a -m "Improve code" -q --allow-empty
```

``` (stderr)
$ git push rad
✓ Patch 590b60d updated to revision ac91e388a85ec9f252f789a3a494c00512c23aa0
To compare against your previous revision 590b60d, run:

   git range-diff 4c66f0e[..] f63e783[..] c6df2b0[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f63e783..c6df2b0  feature/2 -> patches/590b60dcf7618bd968fc7572131a732b0863bd10
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
$ rad patch show 590b60d
╭──────────────────────────────────────────────────────────╮
│ Title     Add more things                                │
│ Patch     590b60dcf7618bd968fc7572131a732b0863bd10       │
│ Author    alice (you)                                    │
│ Head      c6df2b0a99f8cdb0e9199e5a1bbd0ae793238384       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/2                                      │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ c6df2b0 Improve code                                     │
│ f63e783 Add more things                                  │
├──────────────────────────────────────────────────────────┤
│ ● Revision 590b60d @ [..   ]..f63e783 by alice (you) now │
│ ↑ Revision ac91e38 @ [..   ]..c6df2b0 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

And we can check that all the refs are properly updated in our repository:

```
$ git rev-parse HEAD
c6df2b0a99f8cdb0e9199e5a1bbd0ae793238384
```

```
$ git status --short --branch
## feature/2...rad/patches/590b60dcf7618bd968fc7572131a732b0863bd10
```

```
$ git rev-parse refs/remotes/rad/patches/590b60dcf7618bd968fc7572131a732b0863bd10
c6df2b0a99f8cdb0e9199e5a1bbd0ae793238384
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/patches/590b60dcf7618bd968fc7572131a732b0863bd10
c6df2b0a99f8cdb0e9199e5a1bbd0ae793238384	refs/heads/patches/590b60dcf7618bd968fc7572131a732b0863bd10
```

## Force push

Sometimes, it's necessary to force-push a patch update. For example, if we amended
the commit and want the updated patch to reflect that.

Let's try.

```
$ git commit --amend -m "Amended commit" --allow-empty
[feature/2 08d4f53] Amended commit
 Date: [..]
```

Now let's push to the patch head.

``` (stderr) (fail)
$ git push
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [rejected]        feature/2 -> patches/590b60dcf7618bd968fc7572131a732b0863bd10 (non-fast-forward)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
hint: [..]
hint: [..]
hint: [..]
hint: See the 'Note about fast-forwards' in 'git push --help' for details.
```

The push fails because it's not a fast-forward update. To remedy this, we can
use `--force-with-lease` (or `--force`) to force the update.

``` (stderr)
$ git push --force-with-lease
✓ Patch 590b60d updated to revision f13c6f236dc873254b04423c807d4546c40d0372
To compare against your previous revision ac91e38, run:

   git range-diff 4c66f0e[..] c6df2b0[..] 08d4f53[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + c6df2b0...08d4f53 feature/2 -> patches/590b60dcf7618bd968fc7572131a732b0863bd10 (forced update)
```

That worked. We can see the new revision if we call `rad patch show`:

```
$ rad patch show 590b60d
╭──────────────────────────────────────────────────────────╮
│ Title     Add more things                                │
│ Patch     590b60dcf7618bd968fc7572131a732b0863bd10       │
│ Author    alice (you)                                    │
│ Head      08d4f53f08539731774d33eece1211b17f0d1daf       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  feature/2                                      │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
├──────────────────────────────────────────────────────────┤
│ 08d4f53 Amended commit                                   │
│ f63e783 Add more things                                  │
├──────────────────────────────────────────────────────────┤
│ ● Revision 590b60d @ [..   ]..f63e783 by alice (you) now │
│ ↑ Revision ac91e38 @ [..   ]..c6df2b0 by alice (you) now │
│ ↑ Revision f13c6f2 @ [..   ]..08d4f53 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

## Detached HEAD

In some cases, we may be creating patches from a detached HEAD state, but we
still want to have a tracking branch. We can do this using the `patch.branch`
option.

```
$ git commit --allow-empty -m "Going into detached HEAD"
[feature/2 b8daa3a] Going into detached HEAD
```

``` (stderr)
$ git checkout b8daa3a
Note: switching to 'b8daa3a'.

You are in 'detached HEAD' state. You can look around, make experimental
changes and commit them, and you can discard any commits you make in this
state without impacting any branches by switching back to a branch.

If you want to create a new branch to retain commits you create, you may
do so (now or later) by using -c with the switch command. Example:

  git switch -c <new-branch-name>

Or undo this operation with:

  git switch -

Turn off this advice by setting config variable advice.detachedHead to false

HEAD is now at b8daa3a Going into detached HEAD
$ git push rad HEAD:refs/patches -o patch.branch
✓ Patch 5224ce242221c977b6a57a30a98c56f5267c469c opened
✓ Branch patches/5224ce242221c977b6a57a30a98c56f5267c469c created
hint: to update, run `git push rad patches/5224ce242221c977b6a57a30a98c56f5267c469c`
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

The default name used for the branch is `patches/<patch id>`. So let's checkout
the branch and push a new revision:

``` (stderr)
$ git checkout patches/5224ce242221c977b6a57a30a98c56f5267c469c
Switched to branch 'patches/5224ce242221c977b6a57a30a98c56f5267c469c'
$ git commit --allow-empty -m "Pushing new revision"
$ git push rad
✓ Patch 5224ce2 updated to revision db53780ffb4660c521f23d3d7619bcfb1a6cbe48
To compare against your previous revision 5224ce2, run:

   git range-diff [..] [..] [..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   b8daa3a..2670a59  patches/5224ce242221c977b6a57a30a98c56f5267c469c -> patches/5224ce242221c977b6a57a30a98c56f5267c469c
```

However, we also allow you to name the branch yourself:

``` (stderr)
$ git checkout b8daa3a -q
$ git push rad HEAD:refs/patches -o patch.branch='feature/3'
✓ Patch 5224ce242221c977b6a57a30a98c56f5267c469c opened
✓ Branch feature/3 created
hint: to update, run `git push rad feature/3`
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Let's checkout this branch and also push a new revision:

``` (stderr)
$ git checkout feature/3
Switched to branch 'feature/3'
$ git commit --allow-empty -m "Pushing new revision"
$ git push rad
✓ Patch 5224ce2 updated to revision db53780ffb4660c521f23d3d7619bcfb1a6cbe48
To compare against your previous revision 5224ce2, run:

   git range-diff [..] [..] [..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   b8daa3a..2670a59  feature/3 -> patches/5224ce242221c977b6a57a30a98c56f5267c469c
```

## Empty patch

If we try to open a patch without making any changes to our base branch (`master`),
we should get an error:

``` (stderr) (fail)
$ git push rad master:refs/patches
warn: attempted to create a patch using the commit 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927, but this commit is already included in the base branch
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [remote rejected] master -> refs/patches (patch commits are already included in the base branch)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

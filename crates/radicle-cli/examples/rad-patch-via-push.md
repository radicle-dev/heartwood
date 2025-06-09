# Using `git push` to open patches

Let's checkout a branch, make a commit and push to the magic ref `refs/patches`.
When we push to this ref, a patch is created from our commits.

``` (stderr) RAD_HINT=1
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ git commit -a -m "Add things" -q --allow-empty
$ git push -o patch.message="Add things #1" -o patch.message="See commits for details." rad HEAD:refs/patches
✓ Patch 6035d2f582afbe01ff23ea87528ae523d76875b6 opened
hint: to update, run `git push` or `git push rad -f HEAD:patches/6035d2f582afbe01ff23ea87528ae523d76875b6`
hint: offline push, your node is not running
hint: to sync with the network, run `rad node start`
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can see a patch was created:

```
$ rad patch show 6035d2f582afbe01ff23ea87528ae523d76875b6
╭────────────────────────────────────────────────────╮
│ Title     Add things #1                            │
│ Patch     6035d2f582afbe01ff23ea87528ae523d76875b6 │
│ Author    alice (you)                              │
│ Head      42d894a83c9c356552a57af09ccdbd5587a99045 │
│ Branches  feature/1                                │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
│                                                    │
│ See commits for details.                           │
├────────────────────────────────────────────────────┤
│ 42d894a Add things                                 │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (42d894a) now              │
╰────────────────────────────────────────────────────╯
```

If we check our local branch, we can see its upstream is set to track a remote
branch associated with this patch:

```
$ git branch -vv
* feature/1 42d894a [rad/patches/6035d2f582afbe01ff23ea87528ae523d76875b6] Add things
  master    f2de534 [rad/master] Second commit
```

Let's check that it's up to date with our local head:

```
$ git status --short --branch
## feature/1...rad/patches/6035d2f582afbe01ff23ea87528ae523d76875b6
$ git fetch
$ git push
```

And let's look at our local and remote refs:

```
$ git show-ref
42d894a83c9c356552a57af09ccdbd5587a99045 refs/heads/feature/1
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/heads/master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/remotes/rad/master
42d894a83c9c356552a57af09ccdbd5587a99045 refs/remotes/rad/patches/6035d2f582afbe01ff23ea87528ae523d76875b6
```
```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji 'refs/heads/patches/*'
42d894a83c9c356552a57af09ccdbd5587a99045	refs/heads/patches/6035d2f582afbe01ff23ea87528ae523d76875b6
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/cobs/*'
0656c217f917c3e06234771e9ecae53aba5e173e	refs/cobs/xyz.radicle.id/0656c217f917c3e06234771e9ecae53aba5e173e
6035d2f582afbe01ff23ea87528ae523d76875b6	refs/cobs/xyz.radicle.patch/6035d2f582afbe01ff23ea87528ae523d76875b6
```

We can create another patch:

``` (stderr)
$ git checkout -b feature/2 -q master
$ git commit -a -m "Add more things" -q --allow-empty
$ git push rad HEAD:refs/patches
✓ Patch 95808913573cead52ad7b42c7b475260ec45c4b2 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We see both branches with upstreams now:

```
$ git branch -vv
  feature/1 42d894a [rad/patches/6035d2f582afbe01ff23ea87528ae523d76875b6] Add things
* feature/2 8b0ea80 [rad/patches/95808913573cead52ad7b42c7b475260ec45c4b2] Add more things
  master    f2de534 [rad/master] Second commit
```

And both patches:

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author         Reviews  Head     +   -   Updated │
├───────────────────────────────────────────────────────────────────────────────┤
│ ●  6035d2f  Add things #1    alice   (you)  -        42d894a  +0  -0  now     │
│ ●  9580891  Add more things  alice   (you)  -        8b0ea80  +0  -0  now     │
╰───────────────────────────────────────────────────────────────────────────────╯
```

To update our patch, we simply push commits to the upstream branch:

```
$ git commit -a -m "Improve code" -q --allow-empty
```

``` (stderr)
$ git push rad
✓ Patch 9580891 updated to revision d7040c6c97629c2b94f86fb639bebbff5de39697
To compare against your previous revision 9580891, run:

   git range-diff f2de534[..] 8b0ea80[..] 02bef3f[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   8b0ea80..02bef3f  feature/2 -> patches/95808913573cead52ad7b42c7b475260ec45c4b2
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
$ rad patch show 9580891
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add more things                                           │
│ Patch     95808913573cead52ad7b42c7b475260ec45c4b2                  │
│ Author    alice (you)                                               │
│ Head      02bef3fac41b2f98bb3c02b868a53ddfecb55b5f                  │
│ Branches  feature/2                                                 │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 02bef3f Improve code                                                │
│ 8b0ea80 Add more things                                             │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (8b0ea80) now                               │
│ ↑ updated to d7040c6c97629c2b94f86fb639bebbff5de39697 (02bef3f) now │
╰─────────────────────────────────────────────────────────────────────╯
```

And we can check that all the refs are properly updated in our repository:

```
$ git rev-parse HEAD
02bef3fac41b2f98bb3c02b868a53ddfecb55b5f
```

```
$ git status --short --branch
## feature/2...rad/patches/95808913573cead52ad7b42c7b475260ec45c4b2
```

```
$ git rev-parse refs/remotes/rad/patches/95808913573cead52ad7b42c7b475260ec45c4b2
02bef3fac41b2f98bb3c02b868a53ddfecb55b5f
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/patches/95808913573cead52ad7b42c7b475260ec45c4b2
02bef3fac41b2f98bb3c02b868a53ddfecb55b5f	refs/heads/patches/95808913573cead52ad7b42c7b475260ec45c4b2
```

## Force push

Sometimes, it's necessary to force-push a patch update. For example, if we amended
the commit and want the updated patch to reflect that.

Let's try.

```
$ git commit --amend -m "Amended commit" --allow-empty
[feature/2 9304dbc] Amended commit
 Date: [..]
```

Now let's push to the patch head.

``` (stderr) (fail)
$ git push
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [rejected]        feature/2 -> patches/95808913573cead52ad7b42c7b475260ec45c4b2 (non-fast-forward)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
hint: [..]
hint: [..]
hint: [..]
hint: See the 'Note about fast-forwards' in 'git push --help' for details.
```

The push fails because it's not a fast-forward update. To remedy this, we can
use `--force` to force the update.

``` (stderr)
$ git push --force
✓ Patch 9580891 updated to revision 670d02794aa05afd6e0851f4aa848bc87c4712c7
To compare against your previous revision d7040c6, run:

   git range-diff f2de534[..] 02bef3f[..] 9304dbc[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 02bef3f...9304dbc feature/2 -> patches/95808913573cead52ad7b42c7b475260ec45c4b2 (forced update)
```

That worked. We can see the new revision if we call `rad patch show`:

```
$ rad patch show 9580891
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add more things                                           │
│ Patch     95808913573cead52ad7b42c7b475260ec45c4b2                  │
│ Author    alice (you)                                               │
│ Head      9304dbc445925187994a7a93222a3f8bde73b785                  │
│ Branches  feature/2                                                 │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 9304dbc Amended commit                                              │
│ 8b0ea80 Add more things                                             │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (8b0ea80) now                               │
│ ↑ updated to d7040c6c97629c2b94f86fb639bebbff5de39697 (02bef3f) now │
│ ↑ updated to 670d02794aa05afd6e0851f4aa848bc87c4712c7 (9304dbc) now │
╰─────────────────────────────────────────────────────────────────────╯
```

## Empty patch

If we try to open a patch without making any changes to our base branch (`master`),
we should get an error:

``` (stderr) (fail)
$ git push rad master:refs/patches
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [remote rejected] master -> refs/patches (patch commits are already included in the base branch)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

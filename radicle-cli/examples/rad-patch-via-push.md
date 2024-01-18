# Using `git push` to open patches

Let's checkout a branch, make a commit and push to the magic ref `refs/patches`.
When we push to this ref, a patch is created from our commits.

``` (stderr) RAD_HINT=1
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ git commit -a -m "Add things" -q --allow-empty
$ git push -o patch.message="Add things #1" -o patch.message="See commits for details." rad HEAD:refs/patches
✓ Patch d43d4eeffedad8411d9e077208464156fd2fe1f0 opened
hint: to update, run `git push` or `git push rad -f HEAD:patches/d43d4eeffedad8411d9e077208464156fd2fe1f0`
hint: offline push, your node is not running
hint: to sync with the network, run `rad node start`
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We can see a patch was created:

```
$ rad patch show d43d4eeffedad8411d9e077208464156fd2fe1f0
╭────────────────────────────────────────────────────╮
│ Title     Add things #1                            │
│ Patch     d43d4eeffedad8411d9e077208464156fd2fe1f0 │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      42d894a83c9c356552a57af09ccdbd5587a99045 │
│ Branches  feature/1                                │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
│                                                    │
│ See commits for details.                           │
├────────────────────────────────────────────────────┤
│ 42d894a Add things                                 │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (42d894a) now    │
╰────────────────────────────────────────────────────╯
```

If we check our local branch, we can see its upstream is set to track a remote
branch associated with this patch:

```
$ git branch -vv
* feature/1 42d894a [rad/patches/d43d4eeffedad8411d9e077208464156fd2fe1f0] Add things
  master    f2de534 [rad/master] Second commit
```

Let's check that it's up to date with our local head:

```
$ git status --short --branch
## feature/1...rad/patches/d43d4eeffedad8411d9e077208464156fd2fe1f0
$ git fetch
$ git push
```

And let's look at our local and remote refs:

```
$ git show-ref
42d894a83c9c356552a57af09ccdbd5587a99045 refs/heads/feature/1
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/heads/master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/remotes/rad/master
42d894a83c9c356552a57af09ccdbd5587a99045 refs/remotes/rad/patches/d43d4eeffedad8411d9e077208464156fd2fe1f0
```
```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji 'refs/heads/patches/*'
42d894a83c9c356552a57af09ccdbd5587a99045	refs/heads/patches/d43d4eeffedad8411d9e077208464156fd2fe1f0
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/cobs/*'
0656c217f917c3e06234771e9ecae53aba5e173e	refs/cobs/xyz.radicle.id/0656c217f917c3e06234771e9ecae53aba5e173e
d43d4eeffedad8411d9e077208464156fd2fe1f0	refs/cobs/xyz.radicle.patch/d43d4eeffedad8411d9e077208464156fd2fe1f0
```

We can create another patch:

``` (stderr)
$ git checkout -b feature/2 -q master
$ git commit -a -m "Add more things" -q --allow-empty
$ git push rad HEAD:refs/patches
✓ Patch 178c07e0b0ae3cc082438952085397d2aae9500a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

We see both branches with upstreams now:

```
$ git branch -vv
  feature/1 42d894a [rad/patches/d43d4eeffedad8411d9e077208464156fd2fe1f0] Add things
* feature/2 8b0ea80 [rad/patches/178c07e0b0ae3cc082438952085397d2aae9500a] Add more things
  master    f2de534 [rad/master] Second commit
```

And both patches:

```
$ rad patch
╭───────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title            Author                  Head     +   -   Updated │
├───────────────────────────────────────────────────────────────────────────────┤
│ ●  178c07e  Add more things  z6MknSL…StBU8Vi  (you)  8b0ea80  +0  -0  now     │
│ ●  d43d4ee  Add things #1    z6MknSL…StBU8Vi  (you)  42d894a  +0  -0  now     │
╰───────────────────────────────────────────────────────────────────────────────╯
```

To update our patch, we simply push commits to the upstream branch:

```
$ git commit -a -m "Improve code" -q --allow-empty
```

``` (stderr)
$ git push rad
✓ Patch 178c07e updated to revision f82a63ebfcda8b43f2119d5fcadbe52e07c3272e
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   8b0ea80..02bef3f  feature/2 -> patches/178c07e0b0ae3cc082438952085397d2aae9500a
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
$ rad patch show 178c07e
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add more things                                           │
│ Patch     178c07e0b0ae3cc082438952085397d2aae9500a                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      02bef3fac41b2f98bb3c02b868a53ddfecb55b5f                  │
│ Branches  feature/2                                                 │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 02bef3f Improve code                                                │
│ 8b0ea80 Add more things                                             │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (8b0ea80) now                     │
│ ↑ updated to f82a63ebfcda8b43f2119d5fcadbe52e07c3272e (02bef3f) now │
╰─────────────────────────────────────────────────────────────────────╯
```

And we can check that all the refs are properly updated in our repository:

```
$ git rev-parse HEAD
02bef3fac41b2f98bb3c02b868a53ddfecb55b5f
```

```
$ git status --short --branch
## feature/2...rad/patches/178c07e0b0ae3cc082438952085397d2aae9500a
```

```
$ git rev-parse refs/remotes/rad/patches/178c07e0b0ae3cc082438952085397d2aae9500a
02bef3fac41b2f98bb3c02b868a53ddfecb55b5f
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi refs/heads/patches/178c07e0b0ae3cc082438952085397d2aae9500a
02bef3fac41b2f98bb3c02b868a53ddfecb55b5f	refs/heads/patches/178c07e0b0ae3cc082438952085397d2aae9500a
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
 ! [rejected]        feature/2 -> patches/178c07e0b0ae3cc082438952085397d2aae9500a (non-fast-forward)
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
✓ Patch 178c07e updated to revision d125e0b78e1e496d7653d220d09354b1b434a3ab
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 02bef3f...9304dbc feature/2 -> patches/178c07e0b0ae3cc082438952085397d2aae9500a (forced update)
```

That worked. We can see the new revision if we call `rad patch show`:

```
$ rad patch show 178c07e
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Add more things                                           │
│ Patch     178c07e0b0ae3cc082438952085397d2aae9500a                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      9304dbc445925187994a7a93222a3f8bde73b785                  │
│ Branches  feature/2                                                 │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 9304dbc Amended commit                                              │
│ 8b0ea80 Add more things                                             │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (8b0ea80) now                     │
│ ↑ updated to f82a63ebfcda8b43f2119d5fcadbe52e07c3272e (02bef3f) now │
│ ↑ updated to d125e0b78e1e496d7653d220d09354b1b434a3ab (9304dbc) now │
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

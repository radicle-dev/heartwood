```
$ git checkout -b alice/1
$ git commit -m "Alice's commit" --allow-empty -s
[alice/1 87fa120] Alice's commit
```

``` (stderr) RAD_SOCKET=/dev/null
$ git push rad HEAD:alice/1
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      HEAD -> alice/1
```

Make sure we can't force-push without `+`:

``` (stderr)
$ git commit --amend -m "Alice's amended commit" --allow-empty -s
```
``` (stderr) (fail)
$ git push rad HEAD:alice/1
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [rejected]        HEAD -> alice/1 (non-fast-forward)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
hint: [..]
hint: [..]
hint: [..]
hint: See the 'Note about fast-forwards' in 'git push --help' for details.
```

And that we can with `+`:

``` (stderr)
$ git push -o no-sync rad +HEAD:alice/1
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 87fa120...145e1e6 HEAD -> alice/1 (forced update)
```

Notice that we used the `-o no-sync` push option to disable syncing after the push.

```
$ git branch -r -vv
  rad/alice/1 145e1e6 Alice's amended commit
  rad/master  f2de534 Second commit
```

List our namespaced refs:

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/heads/*'
145e1e69bef3ad93d14946ea212249c2fa9b9828	refs/heads/alice/1
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

List the canonical refs:

```
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

```
$ rad sync --announce
✓ Synced with 1 node(s)
```

Note that it is forbidden to delete the default/canonical branch:

``` (fail) (stderr)
$ git push rad :master
error: refusing to delete default branch ref 'refs/heads/master'
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

If you pass an unsupported push option, you get an error:

``` (stderr) (fail)
$ git push -o alien rad HEAD:alice/2
error: unknown push option "alien"
```

We can also push a SHA-1:

```
$ git commit -m "Something good" --allow-empty -s
[alice/1 ddcc1f1] Something good
```
``` (stderr)
$ git push -o no-sync rad ddcc1f164eacfd7dba41da9bff3261da3ee79fd3:refs/heads/alice/2
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      ddcc1f164eacfd7dba41da9bff3261da3ee79fd3 -> alice/2
```

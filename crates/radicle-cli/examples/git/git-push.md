```
$ git checkout -b alice/1
$ git commit -m "Alice's commit" --allow-empty -s
[alice/1 be30b8c] Alice's commit
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
 + be30b8c...9d6273e HEAD -> alice/1 (forced update)
```

Notice that we used the `-o no-sync` push option to disable syncing after the push.

```
$ git branch -r -vv
  rad/alice/1 9d6273e Alice's amended commit
  rad/master  4c66f0e Second commit
```

List our namespaced refs:

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi 'refs/heads/*'
9d6273e3d5393db7c029f03c7b61d3923bb7366b	refs/heads/alice/1
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	refs/heads/master
```

List the canonical refs:

```
$ git ls-remote rad
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	HEAD
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	refs/heads/master
```

```
$ rad sync --announce
✓ Synced with 1 seed(s)
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
[alice/1 d59fd22] Something good
```
``` (stderr)
$ git push -o no-sync rad d59fd22:refs/heads/alice/2
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      d59fd22 -> alice/2
```

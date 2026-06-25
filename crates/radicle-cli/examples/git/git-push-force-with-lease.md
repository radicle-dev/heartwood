Here we show that the Radicle remote helper supports the use of
`--force-with-lease`[^1].

First we will set things up by pushing an initial commit:

```
$ git commit -m "New changes" --allow-empty -q
$ git push rad master
```

Now, we will create a new commit, and use the `--force-with-lease`, which should
succeed. In fact, since the current setup ensures that you can only push to your
namespace, `--force-with-lease` should always work! No other person should be
able to push to your namespace, and so the commit should never have changed from
the last time you pushed.

``` (stderr)
$ git commit --amend -m "Neue Änderungen" --allow-empty -q
$ git push rad master --force-with-lease
✓ Canonical reference refs/heads/master updated to target commit 4014d28cbfbbe6f9b9cd701af852296fab715c12
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 673e76e...4014d28 master -> master (forced update)
```

As per the documentation, you can also pass the reference name, as the expected
value, to `--force-push-lease`:

``` (stderr)
$ git commit --amend -m "Noch mehr Änderungen" --allow-empty -q
$ git push rad master --force-with-lease=master
✓ Canonical reference refs/heads/master updated to target commit 5a68e215f4ac2199fd1b95cecd5d4ca7c47cbc84
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 4014d28...5a68e21 master -> master (forced update)
```

As well as the named reference, and its expected value:

``` (stderr)
$ git commit --amend -m "Even more changes" --allow-empty -q
$ git push rad master --force-with-lease=master:5a68e215f4ac2199fd1b95cecd5d4ca7c47cbc84
✓ Canonical reference refs/heads/master updated to target commit a98bbcfcc6ecf8db902c725af371dc4fd8200e0f
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 5a68e21...a98bbcf master -> master (forced update)
```

If we try use the same expected value as the last push, it should fail since the
reference was updated in the last commit:

```
$ git commit --amend -m "And even more" --allow-empty -q
```

``` (stderr) (fail)
$ git push rad master --force-with-lease=master:5a68e215f4ac2199fd1b95cecd5d4ca7c47cbc84
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [rejected]        master -> master (stale info)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

And if we do not supply the commit, it should also fail, since this implies that
we expect the reference to not exist:

```
$ git commit --amend -m "And even more" --allow-empty -q
```

``` (stderr) (fail)
$ git push rad master --force-with-lease=master:
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 ! [rejected]        master -> master (stale info)
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

So, let's create a new branch:

``` (stderr)
$ git push rad master:dev --force-with-lease=dev:
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      master -> dev
```

[^1]: https://git-scm.com/docs/git-push#Documentation/git-push.txt---force-with-lease

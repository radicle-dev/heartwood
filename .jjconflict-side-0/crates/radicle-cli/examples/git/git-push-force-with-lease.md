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
✓ Canonical reference refs/heads/master updated to target commit 9170c8795d3a78f0381a0ffafb20ea69fb0f5b6b
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + fb25886...9170c87 master -> master (forced update)
```

As per the documentation, you can also pass the reference name, as the expected
value, to `--force-push-lease`:

``` (stderr)
$ git commit --amend -m "Noch mehr Änderungen" --allow-empty -q
$ git push rad master --force-with-lease=master
✓ Canonical reference refs/heads/master updated to target commit 1e4213811eb4ce67360e4a0222cab81ad11a7ffe
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 9170c87...1e42138 master -> master (forced update)
```

As well as the named reference, and its expected value:

``` (stderr)
$ git commit --amend -m "Even more changes" --allow-empty -q
$ git push rad master --force-with-lease=master:1e4213811eb4ce67360e4a0222cab81ad11a7ffe
✓ Canonical reference refs/heads/master updated to target commit c4b74ef30953598852a82e0cd22b2ebb0d8d9e18
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 1e42138...c4b74ef master -> master (forced update)
```

If we try use the same expected value as the last push, it should fail since the
reference was updated in the last commit:

```
$ git commit --amend -m "And even more" --allow-empty -q
```

``` (stderr) (fail)
$ git push rad master --force-with-lease=master:1e4213811eb4ce67360e4a0222cab81ad11a7ffe
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

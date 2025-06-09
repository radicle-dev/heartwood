In this scenario, we will explore being able to rollback to a previous commit.

First we add a second delegate, Bob, to our repo. We also change the threshold
to 2:

``` ~alice
$ rad id update --title "Add Bob" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --threshold 2 -q
069e7d58faa9a7473d27f5510d676af33282796f
```

Bob then syncs these changes and adds a new commit:

``` ~bob
$ rad sync --fetch
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSL…StBU8Vi
$ git commit -m "Third commit" --allow-empty -q
$ git push rad
$ git branch -arv
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master f2de534 Second commit
  rad/master                                                    319a7dc Third commit
```

Alice merges these changes and pushes them, which updates the canonical head:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --fetch --no-sync
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
$ git merge bob/master
Updating f2de534..319a7dc
Fast-forward
```

``` ~alice (stderr)
$ git push rad
✓ Canonical head updated to 319a7dc3b195368ded4b099f8c90bbb80addccd3
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..319a7dc  master -> master
```

Alice decides that she changes her mind about these changes and rolls back to
the previous commit:

``` ~alice
$ git reset --hard f2de534
HEAD is now at f2de534 Second commit
```

Since the canonical head is still decidable from this commit she is allowed to
push and the new canonical head becomes the previous commit again:

``` ~alice (stderr)
$ git push rad -f
✓ Canonical head updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 319a7dc...f2de534 master -> master (forced update)
```

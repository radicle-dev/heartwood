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
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ git commit -m "Third commit" --allow-empty -q
$ git push rad
$ git branch -arv
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master 4c66f0e Second commit
  rad/master                                                    6701ccf Third commit
```

Alice merges these changes and pushes them, which updates the canonical head:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --fetch --no-sync
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git merge bob/master
Updating 4c66f0e..6701ccf
Fast-forward
```

``` ~alice (stderr)
$ git push rad
✓ Canonical reference refs/heads/master updated to target commit 6701ccf21c199b3283ffef64a05bade08adf7987
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..6701ccf  master -> master
```

Alice decides that she changes her mind about these changes and rolls back to
the previous commit:

``` ~alice
$ git reset --hard 4c66f0e
HEAD is now at 4c66f0e Second commit
```

Since the canonical head is still decidable from this commit she is allowed to
push and the new canonical head becomes the previous commit again:

``` ~alice (stderr)
$ git push rad -f
✓ Canonical reference refs/heads/master updated to target commit 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 6701ccf...4c66f0e master -> master (forced update)
```

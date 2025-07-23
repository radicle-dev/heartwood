Let's see what happens if we try to push a head which diverges from the
canonical head.

First we add a second delegate, Bob, to our repo:

``` ~alice
$ rad id update --title "Add Bob" --description "" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji -q
c036c0d89ce26aef3ad7da402157dba16b5163b4
```

Then, as Bob, we commit some code on top of the canonical head:

``` ~bob
$ rad sync --fetch
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ rad inspect --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
$ git commit -m "Third commit" --allow-empty -q
$ git push rad
$ git branch -arv
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master f2de534 Second commit
  rad/master                                                    319a7dc Third commit
```

As Alice, we fetch that code, but commit on top of our own master, which is no
longer canonical, since Bob pushed a more recent commit, and the threshold is 1:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --fetch --no-sync
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git branch -arv
  bob/master 319a7dc Third commit
  rad/master f2de534 Second commit
$ git commit -m "Third commit by Alice" --allow-empty -q
```

If we try to push now, we get an error with a hint, telling us that we need to
integrate Bob's changes before pushing ours:

``` ~alice (stderr)
$ git push rad
warn: could not determine target commit for canonical reference 'refs/heads/master', found diverging commits 2e8758fc512cbdef298c86feddff5ba3280e94b4 and 319a7dc3b195368ded4b099f8c90bbb80addccd3, with base commit f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 and threshold 1
warn: it is recommended to find a commit to agree upon
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..2e8758f  master -> master
```

We do that, and notice that we're now able to push our code:

``` ~alice
$ git pull bob master --rebase
$ git log --oneline
f6cff86 Third commit by Alice
319a7dc Third commit
f2de534 Second commit
08c788d Initial commit
```
``` ~alice RAD_SOCKET=/dev/null (stderr)
$ git push rad -f
✓ Canonical reference refs/heads/master updated to target commit f6cff86594495e9beccfeda7c20173e55c1dd9fc
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + 2e8758f...f6cff86 master -> master (forced update)
```

One thing of note is that we can revert to an older commit as long as we are
still ahead of the other delegates.

``` ~alice
$ git reset --hard HEAD^ -q
```
``` ~alice RAD_SOCKET=/dev/null (stderr)
$ git push -f
✓ Canonical reference refs/heads/master updated to target commit 319a7dc3b195368ded4b099f8c90bbb80addccd3
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + f6cff86...319a7dc master -> master (forced update)
```


Let's see what happens if we try to push a head which diverges from the
canonical head.

First we add a second delegate, Bob, to our repo:

``` ~alice
$ rad id update --title "Add Bob" --description "" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 -q
f48a2c516aceccde576d9ba8845b21eca1f7902c
```

Then, as Bob, we commit some code on top of the canonical head:

``` ~bob
$ rad sync --fetch
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
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
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --fetch --no-sync
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
$ git branch -arv
  bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master 319a7dc Third commit
  rad/master                                                  f2de534 Second commit
$ git commit -m "Third commit by Alice" --allow-empty -q
```

If we try to push now, we get an error with a hint, telling us that we need to
integrate Bob's changes before pushing ours:

``` ~alice (stderr) (fail) RAD_HINT=1
$ git push rad
hint: you are attempting to push a commit that would cause your upstream to diverge from the canonical reference refs/heads/master
hint: to integrate the remote changes, run `git pull --rebase` and try again
error: refusing to update branch to commit that is not a descendant of canonical head
error: failed to push some refs to 'rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

We do that, and notice that we're now able to push our code:

``` ~alice
$ git pull --rebase
$ git log --oneline
f6cff86 Third commit by Alice
319a7dc Third commit
f2de534 Second commit
08c788d Initial commit
```
``` ~alice RAD_SOCKET=/dev/null (stderr)
$ git push rad
✓ Canonical head for refs/heads/master updated to f6cff86594495e9beccfeda7c20173e55c1dd9fc
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..f6cff86  master -> master
```

One thing of note is that we can revert to an older commit as long as we are
still ahead of the other delegates.

``` ~alice
$ git reset --hard HEAD^ -q
```
``` ~alice RAD_SOCKET=/dev/null (stderr)
$ git push -f
✓ Canonical head for refs/heads/master updated to 319a7dc3b195368ded4b099f8c90bbb80addccd3
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + f6cff86...319a7dc master -> master (forced update)
```

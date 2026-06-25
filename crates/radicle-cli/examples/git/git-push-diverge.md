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
  alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master 4c66f0e Second commit
  rad/master                                                    6701ccf Third commit
```

As Alice, we fetch that code, but commit on top of our own master, which is no
longer canonical, since Bob pushed a more recent commit, and the threshold is 1:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob --fetch --no-sync
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git branch -arv
  bob/master 6701ccf Third commit
  rad/master 4c66f0e Second commit
$ git commit -m "Third commit by Alice" --allow-empty -q
```

If we try to push now, we get an error with a hint, telling us that we need to
integrate Bob's changes before pushing ours:

``` ~alice (stderr)
$ git push rad
warn: could not determine target commit for canonical reference 'refs/heads/master', found diverging commits 6701ccf21c199b3283ffef64a05bade08adf7987 and b3cf6bdb16c2b8817f680a9e4aee8d9daf7f1506, with base commit 4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927 and threshold 1
warn: it is recommended to find a commit to agree upon
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   4c66f0e..b3cf6bd  master -> master
```

We do that, and notice that we're now able to push our code:

``` ~alice
$ git pull bob master --rebase
$ git log --oneline
a1854a8 Third commit by Alice
6701ccf Third commit
4c66f0e Second commit
60d31e8 Initial commit
```
``` ~alice RAD_SOCKET=/dev/null (stderr)
$ git push rad -f
✓ Canonical reference refs/heads/master updated to target commit a1854a87e089558043dcfac2eaeb28247dd8afdb
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + b3cf6bd...a1854a8 master -> master (forced update)
```

One thing of note is that we can revert to an older commit as long as we are
still ahead of the other delegates.

``` ~alice
$ git reset --hard HEAD^ -q
```
``` ~alice RAD_SOCKET=/dev/null (stderr)
$ git push -f
✓ Canonical reference refs/heads/master updated to target commit 6701ccf21c199b3283ffef64a05bade08adf7987
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + a1854a8...6701ccf master -> master (forced update)
```

Testing pulling, fetching and the `FETCH_HEAD`.

``` ~bob
$ git push rad
$ git checkout -b bob/1 -q
$ git commit --allow-empty -m "Changes #1" -q
$ git push -o patch.message="Changes" rad HEAD:refs/patches
```

``` ~alice
$ git checkout -b alice/1 -q
$ git rev-parse HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
$ git checkout master -q
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title    Author                   Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ●  74aa72b  Changes  bob     z6Mkt67…v4N1tRk  -        8d5f1ba  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────╯
$ rad patch checkout 74aa72b
✓ Switched to branch patch/74aa72b at revision 74aa72b
✓ Branch patch/74aa72b setup to track rad/patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkt67…v4N1tRk@[..]..
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
$ git checkout master -q
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	not-for-merge	branch 'master' of rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5' of rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

``` ~alice (stderr)
$ git checkout patch/74aa72b -q
$ git commit --allow-empty -m "Changes #2" -q
$ git push
✓ Patch 74aa72b updated to revision 2bc70f5c1d567db16df991a10a618733f3e29d82
To compare against your previous revision 74aa72b, run:

   git range-diff f2de534[..] 8d5f1ba[..] c2aaf1c[..]

✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/74aa72b -> patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5
```

``` ~bob
$ git commit --allow-empty -m "Changes #2" -q
$ git push
```

``` ~alice (stderr)
$ git checkout master -q
$ git pull
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
 + c2aaf1c...8d5f1ba patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5 -> rad/patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5  (forced update)
$ git checkout - -q
$ git commit --allow-empty -m "Changes #3" -q
$ git push
✓ Patch 74aa72b updated to revision c541164492cae34700c601bbe5fdf068183a7d6f
To compare against your previous revision 74aa72b, run:

   git range-diff f2de534[..] 8d5f1ba[..] d9f8caf[..]

✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   c2aaf1c..d9f8caf  patch/74aa72b -> patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5
```

``` ~alice
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354		branch 'master' of rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5' of rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
```

``` ~bob (stderr)
$ git checkout master -q
$ git pull
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
 + c2aaf1c...8d5f1ba patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5 -> rad/patches/74aa72bbd79a88dcb5ae98bb6d06bb493b5ed4c5  (forced update)
```

``` ~bob
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

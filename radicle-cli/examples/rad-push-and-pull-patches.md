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
$ rad patch checkout d004b67
✓ Switched to branch patch/d004b67 at revision d004b67
✓ Branch patch/d004b67 setup to track rad/patches/d004b67355456c46de10c0d287e4a791ad1a6945
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
$ git checkout master -q
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	not-for-merge	branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/d004b67355456c46de10c0d287e4a791ad1a6945' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

``` ~alice (stderr)
$ git checkout patch/d004b67 -q
$ git commit --allow-empty -m "Changes #2" -q
$ git push
✓ Patch d004b67 updated to revision 2eb705c3da98e05c083df15be5b1bd6856a0bd77
To compare against your previous revision d004b67, run:

   git range-diff f2de534[..] 8d5f1ba[..] c2aaf1c[..]

✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/d004b67 -> patches/d004b67355456c46de10c0d287e4a791ad1a6945
```

``` ~bob
$ git commit --allow-empty -m "Changes #2" -q
$ git push
```

``` ~alice (stderr)
$ git checkout master -q
$ git pull
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + c2aaf1c...8d5f1ba patches/d004b67355456c46de10c0d287e4a791ad1a6945 -> rad/patches/d004b67355456c46de10c0d287e4a791ad1a6945  (forced update)
$ git checkout - -q
$ git commit --allow-empty -m "Changes #3" -q
$ git push
✓ Patch d004b67 updated to revision 7b5015a8dac188bb0d44a334aa68a51298750b07
To compare against your previous revision d004b67, run:

   git range-diff f2de534[..] 8d5f1ba[..] d9f8caf[..]

✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   c2aaf1c..d9f8caf  patch/d004b67 -> patches/d004b67355456c46de10c0d287e4a791ad1a6945
```

``` ~alice
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354		branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/d004b67355456c46de10c0d287e4a791ad1a6945' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

``` ~bob (stderr)
$ git checkout master -q
$ git pull
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + c2aaf1c...8d5f1ba patches/d004b67355456c46de10c0d287e4a791ad1a6945 -> rad/patches/d004b67355456c46de10c0d287e4a791ad1a6945  (forced update)
```

``` ~bob
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

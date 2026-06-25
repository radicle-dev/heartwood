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
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
$ git checkout master -q
$ rad patch checkout 6b8ee2f
✓ Switched to branch patch/6b8ee2f at revision 6b8ee2f
✓ Branch patch/6b8ee2f setup to track rad/patches/6b8ee2f4431071b562106dcf6e77aedd9335f169
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git checkout master -q
$ cat .git/FETCH_HEAD
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	not-for-merge	branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
744dc00b19d9e3261b8e98f78d4a72ca39271ef3	not-for-merge	branch 'patches/6b8ee2f4431071b562106dcf6e77aedd9335f169' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git rev-parse master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
```

``` ~alice (stderr)
$ git checkout patch/6b8ee2f -q
$ git commit --allow-empty -m "Changes #2" -q
$ git push
✓ Patch 6b8ee2f updated to revision 6c477034b8beb0be091dc1ada021a80c5b98e7b7
To compare against your previous revision 6b8ee2f, run:

   git range-diff 4c66f0e[..] 744dc00[..] 7bc878b[..]

✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/6b8ee2f -> patches/6b8ee2f4431071b562106dcf6e77aedd9335f169
```

``` ~bob
$ git commit --allow-empty -m "Changes #2" -q
$ git push
```

``` ~alice (stderr)
$ git checkout master -q
$ git pull
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + 7bc878b...744dc00 patches/6b8ee2f4431071b562106dcf6e77aedd9335f169 -> rad/patches/6b8ee2f4431071b562106dcf6e77aedd9335f169  (forced update)
$ git checkout - -q
$ git commit --allow-empty -m "Changes #3" -q
$ git push
✓ Patch 6b8ee2f updated to revision 1845cff9f4bea146fab6c2be9203480143dc6852
To compare against your previous revision 6b8ee2f, run:

   git range-diff 4c66f0e[..] 744dc00[..] b2ad46f[..]

✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   7bc878b..b2ad46f  patch/6b8ee2f -> patches/6b8ee2f4431071b562106dcf6e77aedd9335f169
```

``` ~alice
$ cat .git/FETCH_HEAD
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927		branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
744dc00b19d9e3261b8e98f78d4a72ca39271ef3	not-for-merge	branch 'patches/6b8ee2f4431071b562106dcf6e77aedd9335f169' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

``` ~bob (stderr)
$ git checkout master -q
$ git pull
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + 7bc878b...744dc00 patches/6b8ee2f4431071b562106dcf6e77aedd9335f169 -> rad/patches/6b8ee2f4431071b562106dcf6e77aedd9335f169  (forced update)
```

``` ~bob
$ git rev-parse master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927
```

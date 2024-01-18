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
$ rad patch checkout 4895eaa
✓ Switched to branch patch/4895eaa
✓ Branch patch/4895eaa setup to track rad/patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
$ git checkout master -q
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	not-for-merge	branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

``` ~alice (stderr)
$ git checkout patch/4895eaa -q
$ git commit --allow-empty -m "Changes #2" -q
$ git push
✓ Patch 4895eaa updated to revision aa9f560826c79344536fae8770687b9d4fbb99cd
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/4895eaa -> patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d
```

``` ~bob
$ git commit --allow-empty -m "Changes #2" -q
$ git push
```

``` ~alice (stderr)
$ git checkout master -q
$ git pull
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + c2aaf1c...8d5f1ba patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d -> rad/patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d  (forced update)
$ git checkout - -q
$ git commit --allow-empty -m "Changes #3" -q
$ git push
✓ Patch 4895eaa updated to revision 9f997dc90ecc5e8a0562a81356ed0432fe3bdaf8
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   c2aaf1c..d9f8caf  patch/4895eaa -> patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d
```

``` ~alice
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354		branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

``` ~bob (stderr)
$ git checkout master -q
$ git pull
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + c2aaf1c...8d5f1ba patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d -> rad/patches/4895eaa8c751e0cd75ce5ef39c65e0cc05b7235d  (forced update)
```

``` ~bob
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

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
$ rad patch checkout 0fd67a0
✓ Switched to branch patch/0fd67a0
✓ Branch patch/0fd67a0 setup to track rad/patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Remote bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk added
✓ Remote-tracking branch bob@z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/master created for z6Mkt67…v4N1tRk
$ git checkout master -q
$ git fetch --all -q
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354		branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	not-for-merge	branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

``` ~alice (stderr)
$ git checkout patch/0fd67a0 -q
$ git commit --allow-empty -m "Changes #2" -q
$ git push
✓ Patch 0fd67a0 updated to c360232989049f6d95efe3512e68608317333a5e
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new branch]      patch/0fd67a0 -> patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21
```

``` ~bob
$ git commit --allow-empty -m "Changes #2" -q
$ git push
```

``` ~alice (stderr)
$ git checkout master -q
$ git pull
✓ Synced with 1 peer(s)
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + c2aaf1c...8d5f1ba patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21 -> rad/patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21  (forced update)
$ git checkout - -q
$ git commit --allow-empty -m "Changes #3" -q
$ git push
✓ Patch 0fd67a0 updated to c4115970191cd0e67212b6d26ad9e3bd992dce35
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   c2aaf1c..d9f8caf  patch/0fd67a0 -> patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21
```

``` ~alice
$ cat .git/FETCH_HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354		branch 'master' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
8d5f1bae4b69d8e3f6cbfc6f4bd675ed19990afc	not-for-merge	branch 'patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21' of rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

``` ~bob (stderr)
$ git checkout master -q
$ git pull
✓ Synced with 1 peer(s)
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
 + c2aaf1c...8d5f1ba patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21 -> rad/patches/0fd67a0364af1f79ed8770a35ed09d85571d4c21  (forced update)
```

``` ~bob
$ git rev-parse master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
```

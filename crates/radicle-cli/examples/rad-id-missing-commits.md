Sometimes, commits may appear missing in the working copy when pushing to the
default branch. In this scenario, we show this happening, and then how to
recover from the problem.

First, we need to be in a scenario where there is more than one delegate:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
7be665f9fccba97abb21b2fa85a6fd3181c72858
```

``` ~alice
$ rad follow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ rad sync
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkt67…v4N1tRk
✓ Synced with 1 seed(s)
$ rad id update --title "Bump threshold" --description "Bumping threshold to 2" --threshold 2 -q
f515dc5af139b8eb9fa817df3f637f2acc29c12b
$ rad sync -a
✓ Synced with 1 seed(s)
```

``` ~bob
$ rad id accept f515dc5af139b8eb9fa817df3f637f2acc29c12b -q
$ rad sync -a
✓ Synced with 1 seed(s)
```

At this stage, Bob makes some changes at the same time, updating the default
branch:

``` ~bob (stderr) RAD_SOCKET=/dev/null
$ touch README.md
$ git add README.md
$ git commit -m "Add README"
$ git push rad master
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   f2de534..361f146  master -> master
```

Alice, is also busy making some changes:

``` ~alice
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkt67…v4N1tRk
```

``` ~alice
$ touch LICENSE
$ git add LICENSE
$ git commit -m "Add LICENSE"
[master 62d19fd] Add LICENSE
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
```

However, when she goes to push to the default branch she sees an error about a missing commit from Bob:

``` ~alice (fails) (stderr)
$ git push rad master
error: the commit 361f146ec7339fffdea1ea586f51410250bec9cf for did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk is missing from the repository [..]
error: failed to push some refs to 'rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi'
```

The reason for this is that when attempting to compute the canonical commit for
the default branch, there are some checks to see if the delegates agree on the
new commit. In this case, Bob's commit was not available to perform this check,
so, Alice must fetch from Bob's state of the repository. She can do this by
adding him as a remote:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name "bob"
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
```


``` ~alice (stderr) RAD_SOCKET=/dev/null
$ git push rad master
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   f2de534..62d19fd  master -> master
```

Note that if the remote tracking branch already exists, then she can simply `git
fetch bob/master`.

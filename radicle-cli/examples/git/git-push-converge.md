In this scenario we check that we can easily reset our canonical head to the
head of another delegate when there is divergence between the 3 delegates.

First we add our new delegates, Bob & Eve, to our repo, while also setting the
`threshold` to `2`:

``` ~alice
$ rad id update --title "Add Bob & Eve" --description "" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --threshold 2 --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji -q
73bd6ea3f88eb0687afd13ee13bfb31a9eb0ccd2
```

Bob and Eve will fetch the changes to ensure they hear about their delegate
responsibilities:

``` ~bob
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetched repository from 2 seed(s)
```

``` ~eve
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 2 seed(s)
```

To demonstrate the divergence, Alice, Bob, and Eve will all create a new change,
pushing to their `rad` remote -- but they won't sync to the network just yet:

``` ~alice
$ git commit -m "Alice's commit" --allow-empty -q
$ git push rad -o no-sync
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

``` ~bob
$ git commit -m "Bob's commit" --allow-empty -q
$ git push rad -o no-sync
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

``` ~eve
$ git commit -m "Eve's commit" --allow-empty -q
$ git push rad -o no-sync
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

Alice adds Bob and Eve as remotes and starts to notice that the `no quorum was
found` error is showing up:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✗ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z.. error: no quorum was found
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
$ rad remote add did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --name eve
✓ Follow policy updated for z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z (eve)
✗ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk.. error: no quorum was found
✗ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z.. error: no quorum was found
✓ Remote eve added
✓ Remote-tracking branch eve/master created for z6Mkux1…nVhib7Z
```

Alice does indeed have Bob and Eve's references, however, a new canonical
`refs/heads/master` cannot be decided while they're out of quorum. This can be
remedied by the delegates agreeing upon which way to move forward. In this case,
Alice resets her `master` to `bob/master`:

``` ~alice
$ git reset bob/master --hard
HEAD is now at 0801f02 Bob's commit
```

She can then force push to update the canonical head to the new agreed upon
commit:

``` ~alice (stderr)
$ git push rad -f
✓ Canonical head updated to 0801f020f11a08ec7a96d1710ec24e5e005499d1
✓ Synced with 2 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + d09e634...0801f02 master -> master (forced update)
```

We can convince ourselves that the canonical branch has indeed changed by using
`git ls-remote` for each of the delegates:

``` ~alice
$ git ls-remote rad
0801f020f11a08ec7a96d1710ec24e5e005499d1	refs/heads/master
```

``` ~bob
$ git ls-remote rad
0801f020f11a08ec7a96d1710ec24e5e005499d1	refs/heads/master
```

``` ~eve
$ git ls-remote rad
0801f020f11a08ec7a96d1710ec24e5e005499d1	refs/heads/master
```




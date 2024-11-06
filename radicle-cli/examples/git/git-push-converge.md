In this scenario we check that we can easily reset our canonical head to the
head of another delegate when there is divergence between the 3 delegates.

First we add our new delegates, Bob & Eve, to our repo, while also setting the
`threshold` to `3`:

``` ~alice
$ rad id update --title "Add Bob & Eve" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --delegate did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --threshold 3 --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji -q
3143236b2e40338f5574ec04e935a5ab80a6868a
```

Bob and Eve will fetch the changes to ensure they hear about their delegate
responsibilities:

``` ~bob
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
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
$ git add README
$ git commit -m "Bob's commit" -q
$ git push rad -o no-sync
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

``` ~eve
$ git add README
$ git commit -m "Eve's commit" -q
$ git push rad -o no-sync
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
```

Alice adds Bob and Eve as remotes and starts to notice that the `no quorum was
found` error is showing up:

``` ~alice
$ rad remote add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
$ rad remote add did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z --name eve
✓ Follow policy updated for z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z (eve)
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Remote eve added
✓ Remote-tracking branch eve/master created for z6Mkux1…nVhib7Z
```

Alice does indeed have Bob and Eve's references, however, a new canonical
`refs/heads/master` cannot be decided while they're out of quorum. This can be
remedied by the delegates agreeing upon which way to move forward. In this case,
Alice resets her `master` to `bob/master`:

``` ~alice
$ git merge bob/master
Merge made by the 'ort' strategy.
 README | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)
$ git merge eve/master -s ours
Merge made by the 'ours' strategy.
```

She can then force push to update the canonical head to the new agreed upon
commit:

``` ~alice (stderr)
$ git push rad -f
warn: could not determine canonical tip for `refs/heads/master`
warn: no commit found with at least 3 vote(s) (threshold not met)
warn: it is recommended to find a commit to agree upon
✓ Synced with 2 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   d09e634..0f9bd80  master -> master
```

``` ~bob
$ rad remote add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --name alice
✓ Follow policy updated for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkux1…nVhib7Z..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Remote alice added
✓ Remote-tracking branch alice/master created for z6MknSL…StBU8Vi
$ git reset --hard alice/master
HEAD is now at 0f9bd80 Merge remote-tracking branch 'eve/master'
```

When we check Bob's `rad` remote, we see that the commit for `refs/heads/master`
has changed. This is actually Eve's commit as part of Alice merging above. Now
that Alice, Bob, and Eve all have this commit as part of their history it has
become the canonical `master`.

``` ~bob (stderr)
$ git push rad
✓ Canonical head updated to 3a75f66dd0020c9a0355cc6ec21f15de989e2001
✓ Synced with 2 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   2a37862..0f9bd80  master -> master
```

Once Eve also resets to the merge commits, the canonical `master` is set to this tip.

``` ~eve
$ rad remote add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --name alice
✓ Follow policy updated for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Remote alice added
✓ Remote-tracking branch alice/master created for z6MknSL…StBU8Vi
$ git reset --hard alice/master
HEAD is now at 0f9bd80 Merge remote-tracking branch 'eve/master'
```

``` ~eve (stderr)
$ git push rad
✓ Canonical head updated to 0f9bd8035c04b3f73f5408e73e8454879b20800b
✓ Synced with 2 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
   3a75f66..0f9bd80  master -> master
```

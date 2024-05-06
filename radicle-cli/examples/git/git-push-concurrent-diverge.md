We want to ensure that concurrent pushes by delegates can be resolved.

So first, we add Bob as a delegate:

``` ~alice
$ rad id update --title "Add Bob" --description "" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji -q
c036c0d89ce26aef3ad7da402157dba16b5163b4
```

``` ~bob
$ rad sync --fetch
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
$ rad inspect --delegates
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
```

Alice and Bob simultaneously add commits (without syncing):

``` ~alice
$ git commit -m "Commit by Alice" --allow-empty -q
$ git push rad -o no-sync
```

``` ~bob
$ git commit -m "Commit by Bob" --allow-empty -q
$ git push rad -o no-sync
```

When Alice attempts to `rad sync` it fails, saying that a quorum could not be
found:

``` ~alice (fails)
$ rad sync
✗ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk.. error: no quorum was found
✗ Error: repository fetch from 1 seed(s) failed
✗ Found 1 seed(s)..
✗ Error: all seeds timed out
```

Alice can reset to the previous commit and use `allow.rollback` to push to her
`master` branch:

``` ~alice
$ git reset HEAD^ --hard
HEAD is now at f2de534 Second commit
$ git push rad -o allow.rollback -f
```

She can successfully `rad sync` again:

``` ~alice
$ rad sync
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6Mkt67…v4N1tRk..
✓ Fetched repository from 1 seed(s)
✓ Nothing to announce, already in sync with 1 node(s) (see `rad sync status`)
```

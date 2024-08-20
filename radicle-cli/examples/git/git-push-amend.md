``` ~alice
$ rad id update --title "Add Bob" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 -q
f48a2c516aceccde576d9ba8845b21eca1f7902c
```

``` ~bob
$ rad sync --fetch
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
```

``` ~alice
$ git commit -m "New changes" --allow-empty -q
$ git push rad master -o no-sync
```

``` ~alice
$ git commit --amend -m "Neue Änderungen" --allow-empty -q
```

``` ~alice (stderr)
$ git push rad master -f
✓ Canonical head for refs/heads/master updated to 9170c8795d3a78f0381a0ffafb20ea69fb0f5b6b
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + fb25886...9170c87 master -> master (forced update)
```

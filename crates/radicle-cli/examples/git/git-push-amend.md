``` ~alice
$ rad id -q --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji update --title "Add Bob" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
c036c0d89ce26aef3ad7da402157dba16b5163b4
```

``` ~bob
$ rad sync --fetch
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
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
✓ Canonical reference refs/heads/master updated to target commit 9170c8795d3a78f0381a0ffafb20ea69fb0f5b6b
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + fb25886...9170c87 master -> master (forced update)
```

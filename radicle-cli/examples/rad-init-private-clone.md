Given a private repo `rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu` belonging to Alice,
Bob tries to clone it, and even though he's connected to Alice, it fails.

``` ~bob
$ rad track rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu
✓ Tracking policy updated for rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu with scope 'trusted'
$ rad ls
```
``` ~bob (fail)
$ rad sync rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --fetch --seed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --timeout 1
✗ Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from z6MknSL…StBU8Vi.. error: connection reset
✗ Error: repository fetch from 1 seed(s) failed
```

She allows Bob to view the repository. And when she syncs, one node (Bob) gets
the refs.

``` ~alice
$ rad id update --title "Allow Bob" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
...
$ rad sync --announce --timeout 3
✓ Synced with 1 node(s)
```

Bob can now fetch the private repo:

``` ~bob
$ rad sync rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --fetch
✓ Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
$ rad ls --private
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu   private      f2de534   radicle heartwood protocol & stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

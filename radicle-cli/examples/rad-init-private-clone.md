Given a private repo `rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu` belonging to Alice,
Bob tries to fetch it, and even though he's connected to Alice, it fails.

``` ~bob
$ rad ls
```
``` ~bob (fail)
$ rad clone rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --seed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --timeout 1
✓ Seeding policy updated for rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu with scope 'all'
Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from the network, found 1 potential seed(s).
✗ Target not met: could not fetch from [z6MknSL…StBU8Vi], and required 1 more seed(s)
! Warning: Failed to fetch from 1 seed(s).
! Warning: z6MknSL…StBU8Vi: failed to perform fetch handshake
✗ Error: no seeds found for rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu
```

She allows Bob to view the repository. And when she syncs, one node (Bob) gets
the refs.

``` ~alice
$ rad id update --title "Allow Bob" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
...
$ rad sync --announce --timeout 3
✓ Synced with 1 node(s)
```

Bob can now fetch the private repo without specifying a seed, because he knows
that Alice has the repo after she announced her refs:

``` ~bob
$ rad sync rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --fetch
Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSL…StBU8Vi
$ rad ls --private --all
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu   private      f2de534   radicle heartwood protocol & stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note that since we don't have our own fork of this repo, omitting the `--all` flag shows nothing:

``` ~bob
$ rad ls --private
Nothing to show.
```

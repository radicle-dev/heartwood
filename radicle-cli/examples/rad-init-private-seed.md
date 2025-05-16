Alice allows Bob to fetch this repo, but doesn't announce it, which means
that Bob needs to know to fetch it from Alice.

``` ~alice
$ rad id update --title "Allow Bob" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
[..]
```

First, Bob seeds the repo.

``` ~bob
$ rad seed rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --no-fetch
[..]
```

If Bob just tries to fetch it without specifying seeds, he gets an error:

``` ~bob (fails)
$ rad sync rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --fetch
✗ Error: no candidate seeds were found to fetch from
```

He has to specify a seed that isn't in his routing table:

``` ~bob
$ rad sync rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --fetch --seed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from the network, found 1 potential seed(s).
✓ Target met: 1 preferred seed(s).
🌱 Fetched from z6MknSL…StBU8Vi
```

``` ~bob
$ rad ls --private --all
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu   private      f2de534   radicle heartwood protocol & stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note that if multiple seeds are specified, the command succeeds as long as one
seed succeeds.

``` ~bob
$ rad sync rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --fetch --seed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --seed z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx
Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from the network, found 1 potential seed(s).
✓ Target met: 1 replica(s)
🌱 Fetched from z6MknSL…StBU8Vi
```

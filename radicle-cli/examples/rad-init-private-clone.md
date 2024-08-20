Given a private repo `rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT` belonging to Alice,
Bob tries to fetch it, and even though he's connected to Alice, it fails.

``` ~bob
$ rad ls
```
``` ~bob (fail)
$ rad clone rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT --seed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --timeout 1
✓ Seeding policy updated for rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT with scope 'all'
✗ Fetching rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT from z6MknSL…StBU8Vi@[..].. error: failed to perform fetch handshake
✗ Error: repository rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT not found
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
$ rad sync rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT --fetch
✓ Fetching rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
$ rad ls --private --all
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                 Visibility   Head      Description                        │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z2gud85wgGxzN7MNvi8wDEBFqLqmT   private      f2de534   radicle heartwood protocol & stack │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note that since we don't have our own fork of this repo, omitting the `--all` flag shows nothing:

``` ~bob
$ rad ls --private
Nothing to show.
```

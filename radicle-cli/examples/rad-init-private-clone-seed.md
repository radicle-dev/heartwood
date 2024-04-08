Given a private repo `rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu` belonging to Alice,
Alice allows Bob to fetch it, and Bob, without the updated identity document
is able to fetch it by specifiying Alice as a seed.

``` ~alice
$ rad id update --title "Allow Bob" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
...
$ rad inspect --identity
{
  "payload": {
    "xyz.radicle.project": {
      "defaultBranch": "master",
      "description": "radicle heartwood protocol & stack",
      "name": "heartwood"
    }
  },
  "delegates": [
    "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"
  ],
  "threshold": 1,
  "visibility": {
    "type": "private",
    "allow": [
      "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
    ]
  }
}
```

``` ~bob
$ rad ls --all --private
$ rad clone rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu --private --seed z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --timeout 1
✓ Seeding policy updated for rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu with scope 'all'
✓ Fetching rad:z2ug5mwNKZB8KGpBDRTrWHAMbvHCu from z6MknSL…StBU8Vi..
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [...]/.radicle/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ radicle heartwood protocol & stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
```

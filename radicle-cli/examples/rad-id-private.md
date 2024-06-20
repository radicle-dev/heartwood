When we are working with a private repository, we can modify the list of peers
we allow by using the `rad id` command with its `--allow` and `--disallow`
options. Both options can be specified multiple times in the same command line call:

Here we will add Bob and Eve's DIDs to the `allow`list:

```
$ rad id update --title "Allow Bob & Eve" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --allow did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z -q
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
      "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
      "did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z"
    ]
  }
}
```

To remove a peer's DID, we can use the `--disallow` option. Let's remove both of them again:

```
$ rad id update --title "Remove allow list" --disallow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --disallow did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
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
    "type": "private"
  }
}
```

Note that using both `--disallow` and `--allow` with the same DID will result in
an error:

``` (fails)
$ rad id update --title "Remove allow list" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --disallow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✗ Error: `--allow` and `--disallow` must not overlap: ["did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"]
```

Allowing or disallowing the same peer twice will result in a message saying that
the document is already up to date:

```
$ rad id update --title "Allow Bob" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
...
```
```
$ rad id update --title "Allow Bob" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
Nothing to do. The document is up to date. See `rad inspect --identity`.
```

If we attempt to change the list while also changing the repository to `public`,
then the command will fail since there is no longer an allow list to work with:

``` (fails)
$ rad id update --visibility public --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✗ Error: `--allow` and `--disallow` cannot be used with `--visibility public`
```

Let's change the repository to `public`:

```
$ rad id update --title "IPO" --visibility public -q
...
```

Now, if we attempt to change the `allow` list we also get an error with a
helpful hint:

``` (fails)
$ rad id update --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✗ Error: `--allow` and `--disallow` should only be used for private repositories
✗ Hint: use `--visibility private` to make the repository private, or perhaps you meant to use `--delegate`/`--rescind`
```


When we are working with a private repository, we can modify the list of peers
we allow by using the `rad id` command with its `--allow` and `--disallow`
options. Both options can be specified multiple times in the same command line call:

Here we will add Bob and Eve's DIDs to the `allow`list:

```
$ rad id update --title "Allow Bob & Eve" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --allow did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z -q
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
$ rad id update --title "Remove allow list" --description "" --disallow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --disallow did:key:z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
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
$ rad id update --title "Remove allow list" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --disallow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✗ Error: --allow and --disallow must have different DIDs: ["did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"]
```

Allowing or disallowing the same peer twice will result in an error the second
call, since there is no update specified:

```
$ rad id update --title "Allow Bob" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
...
```
``` (fails)
$ rad id update --title "Allow Bob" --description "" --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
✗ Error: no update specified
✗ Hint: an update to the identity must be specified, run `rad id update -h` to see the available options
```

If we attempt to change the list while also changing the repository to `public`,
then the command will fail since there is no longer an allow list to work with:

``` (fails)
$ rad id update --visibility public --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✗ Error: --allow and --disallow cannot be used with `--visibility public`
```

Let's change the repository to `public`:

```
$ rad id update --title "IPO" --description "" --visibility public -q
...
```

Now, if we attempt to change the `allow` list we also get an error with a
helpful hint:

``` (fails)
$ rad id update --allow did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
✗ Error: --allow and --disallow should only be used for private repositories
✗ Hint: use `--visibility private` to make the repository private, or perhaps you meant to use `--delegate`/`--rescind`
```


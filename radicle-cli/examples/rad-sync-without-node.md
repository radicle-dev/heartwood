When you try to track, clone, or sync without your node running, it gives you an error:

``` ~alice (fail)
$ rad track rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5 --no-fetch
✗ Error: to track a repository, your node must be running. To start it, run `rad node start`
```

``` ~bob (fail)
$ rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
✗ Error: to clone a repository, your node must be running. To start it, run `rad node start`
```

``` ~eve (fail)
$ rad sync --fetch rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5 --seed z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz
✗ Error: to sync a repository, your node must be running. To start it, run `rad node start`
```

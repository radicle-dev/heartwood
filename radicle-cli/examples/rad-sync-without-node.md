When you try to clone or sync without your node running, it gives you an error:

``` ~bob (fail)
$ rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
✗ Error: to clone a repository, your node must be running. To start it, run `rad node start`
```

``` ~eve (fail)
$ rad sync --fetch rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5 --seed z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz
✗ Error: to sync a repository, your node must be running. To start it, run `rad node start`
```

Note that seeding works fine without a running node:

``` ~alice
$ rad seed rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
✓ Seeding policy updated for rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5 with scope 'all'
```

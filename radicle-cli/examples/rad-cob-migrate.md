``` (fail)
$ rad patch list
✗ Error: collaborative objects database is out of date
✗ Hint: run `rad cob migrate` to update your database
```

``` (fail)
$ rad issue list
✗ Error: collaborative objects database is out of date
✗ Hint: run `rad cob migrate` to update your database
```

```
$ rad cob migrate
✓ Migration [..]/[..] in progress.. (100%)
✓ Migrated collaborative objects database successfully (version = [..])
```

```
$ rad issue list
Nothing to show.
$ rad patch list
Nothing to show.
```

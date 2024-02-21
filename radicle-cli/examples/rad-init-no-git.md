If you try to `init` from a directory that doesn't contain a Git repository,
it will fail:

``` (fail)
$ rad init
✗ Error: a Git repository was not found at the given path
```

Ok so let's initialize one.

```
$ git init -q
```

Now we try again.

``` (fail)
$ rad init
✗ Error: repository head must point to a commit
```

Looks like we need a commit.

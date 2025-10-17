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
✗ Error: could not determine default branch in repository
✗ Hint: perhaps you need to create a branch?
✗ Error: aborting `rad init`
```

Looks like we need to get to work and start working on a branch and add commits
to it.

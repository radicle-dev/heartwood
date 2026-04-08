We initialize Jujutusu for our repository by colocating with Git.

```(stderr)
$ jj git init --colocate
Done importing changes from the underlying Git repo.
Hint: The following remote bookmarks aren't associated with the existing local bookmarks:
  master@rad
Hint: Run `jj bookmark track master@rad` to keep local bookmarks updated on future pulls.
Initialized repo in "."
```
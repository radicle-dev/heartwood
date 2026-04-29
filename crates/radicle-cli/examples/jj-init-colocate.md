We initialize Jujutsu for our repository by colocating with Git.

```(stderr)
$ jj git init --colocate
Done importing changes from the underlying Git repo.
Hint: The following remote bookmarks aren't associated with the existing local bookmarks:
  master@rad
Hint: Run the following command to keep local bookmarks updated on future pulls:
  jj bookmark track master[..]rad
Initialized repo in "."
Hint: Running `git clean -xdf` will remove `.jj/`!
```

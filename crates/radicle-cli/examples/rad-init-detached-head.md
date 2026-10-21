Let's assume you created a Git repository containing two commits and checked out
the first one, leaving you in a detached `HEAD` state:

```
$ git init -q
$ touch file1.txt
$ git add .
$ git commit -m "Create file" -q
$ touch file2.txt
$ git add .
$ git commit -m "Create a second file" -q
$ git checkout HEAD~1
$ git config --local init.defaultBranch nonexistent
```

If you try to `rad init` the repository in this state, it will attempt to
determine a default branch. However, it cannot because we are currently not on
any branch and `init.defaultBranch` does not name an existing local branch.
Setting this option locally overrides any default branch configured by the host,
including the system configuration installed by Git for Windows.

Using `rad init` will fail, providing a hint on how to fix the issue:

``` (fail)
$ rad init
✗ Error: in detached HEAD state
✗ Hint: try `git checkout <default branch>` or set `git config set --local init.defaultBranch <default branch>`
✗ Error: aborting `rad init`
```

Alternatively, if you know a branch that already exists, and you would like to
use, you can use the `--default-branch` option.

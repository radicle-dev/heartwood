Let's make sure that the config is exactly what we expect.

```
$ jj config list
ui.editor = "true"
user.name = "Test User"
user.email = "test.user@example.com"
debug.commit-timestamp = "2001-02-03T04:05:06+07:00"
debug.randomness-seed = 1
debug.operation-timestamp = "2001-02-03T04:05:06+07:00"
operation.hostname = "host.example.com"
operation.username = "test-username"
```

We enable writing Change ID headers to our commits.

```
$ jj config set --user git.write-change-id-header true
```

We initialize Jujutusu for our repository.

```(stderr)
$ jj git init --colocate
Done importing changes from the underlying Git repo.
Hint: The following remote bookmarks aren't associated with the existing local bookmarks:
  master@rad
Hint: Run `jj bookmark track master@rad` to keep local bookmarks updated on future pulls.
Initialized repo in "."
```

Off we go!

```
$ jj status
The working copy has no changes.
Working copy  (@) : pmmvwywv ed64a0f3 (empty) (no description set)
Parent commit (@-): xpnzuzwn f2de534b master master@rad | Second commit
```

Just making sure that Git sees the Change ID…

```
$ git cat-file commit ed64a0f3
tree b4eecafa9be2f2006ce1b709d6857b07069b4608
parent f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
author Test User <test.user@example.com> 981147906 +0700
committer Test User <test.user@example.com> 981147906 +0700
change-id pmmvwywvzvvnvnzntqnqknuzpwttyvkr

```
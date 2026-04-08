Let's make sure that the config is exactly what we expect.

```
$ jj config list
ui.editor = "true"
user.name = "Test User"
user.email = "test.user@example.com"
debug.commit-timestamp = "2001-02-03T04:05:06+07:00"
debug.randomness-seed = 0
debug.operation-timestamp = "2001-02-03T04:05:06+07:00"
operation.hostname = "host.example.com"
operation.username = "test-username"
```

We enable writing Change ID headers to our commits.

```
$ jj config set --user git.write-change-id-header true
```
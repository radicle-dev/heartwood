The `rad sync` command announces changes to the network and waits for other
nodes to be synchronized with those changes.

For instance let's create an issue and sync it with the network:

```
$ rad issue open --title "Test `rad sync`" --description "Check that the command works" -q --no-announce
```

Now let's run `rad sync`. This will announce the issue refs to the network and
wait for nodes to announce that they have fetched those refs.

```
$ rad sync --announce
✓ Synced with 2 node(s)
```

If we try to sync again after the nodes have synced, we will get a timeout
after one second, since the nodes will not emit any message:

``` (fail)
$ rad sync --announce --timeout 1
✗ Syncing with 2 node(s)..
! Seed z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk timed out..
! Seed z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z timed out..
✗ Sync failed: all seeds timed out
```

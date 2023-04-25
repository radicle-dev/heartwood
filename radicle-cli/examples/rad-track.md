To configure our node's tracking policy, we can use the `rad track` command.
For example, let's track a remote node we know about, and alias it to "eve":

```
$ rad track did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --alias eve
✓ Tracking policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (eve)
```

Now let's track one of Eve's repositories:

```
$ rad track rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope trusted --no-fetch
✓ Tracking policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'trusted'
```

Updating the repository identity via `rad cob update` is forbidden:

``` (fail)
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.id --object 0656c217f917c3e06234771e9ecae53aba5e173e --message "Danger" /dev/null
✗ Error: Update of collaborative objects of type xyz.radicle.id is not supported.
```
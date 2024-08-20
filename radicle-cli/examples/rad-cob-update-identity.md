Updating the repository identity via `rad cob update` is forbidden:

``` (fail)
$ rad cob update --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.id --object eeb8b44890570ccf85db7f3cb2a475100a27408a --message "Danger" /dev/null
✗ Error: Update of collaborative objects of type xyz.radicle.id is not supported.
```

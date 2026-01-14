Alice has created a new repository `rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji` with only herself as the sole delegate. After Bob has cloned it, let's ensure he can't add himself as a delegate too:

``` ~bob (fail)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add myself!" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm
✗ Error: store: update error: apply failed: non-delegate not authorized to perform this action
✗ Hint: You are not currently a delegate of this repository!
```


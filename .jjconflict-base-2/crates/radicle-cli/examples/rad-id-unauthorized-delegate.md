Alice has created a new repository `rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji` with only herself as the sole delegate. After Bob has cloned it, let's ensure he can't add himself as a delegate too:

``` ~bob (fail)
$ rad id update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --title "Add myself!" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm
✗ Error: did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk is not a delegate, and only delegates are allowed to create a revision
✗ Hint: bob (you) is attempting to modify the identity document but is not a delegate!
```


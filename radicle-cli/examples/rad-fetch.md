Using `rad clone` is useful if we want to create and fetch a project
that exists on Radicle, but perhaps we're in a scenario where we may
already have an existing Git repository and so a full clone is not
necessary.

Instead, we want to fetch the project from the network into our local
storage. In this scenario, we know that the project is
`rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji`. In order to fetch it, we first
have to track the project.

```
$ rad track rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-fetch
✓ Tracking policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'trusted'
```

Now that the project is tracked we can fetch it and we will have it in
our local storage. Note that the `track` command can also be told to fetch
by passing the `--fetch` option.

```
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
```

However, we don't have a local fork of the project. We can follow this
up with [rad-fork](rad-fork.md).

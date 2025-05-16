Using `rad clone` is useful if we want to create and fetch a project
that exists on Radicle, but perhaps we're in a scenario where we may
already have an existing Git repository and so a full clone is not
necessary.

Instead, we want to fetch the project from the network into our local
storage. In this scenario, we know that the project is
`rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji`. In order to fetch it, we first
have to update our seeding policy for the project.

```
$ rad seed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-fetch
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
```

Now that the project is seeding we can fetch it and we will have it in
our local storage. Note that the `seed` command can also be told to fetch
by passing the `--fetch` option.

```
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 replica(s)
🌱 Fetched from z6MknSL…StBU8Vi
```

However, we don't have a local fork of the project. We can follow this
up with [rad-fork](rad-fork.md).

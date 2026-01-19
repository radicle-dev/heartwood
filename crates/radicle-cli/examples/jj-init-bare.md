We initialize Jujutusu for our repository for use with a bare Git repo.

```(stderr)
$ jj git init --git-repo heartwood heartwood.jj
Initialized repo in "heartwood.jj"
Hint: Running `git clean -xdf` will remove `.jj/`!
```

```
$ cd heartwood.jj
```

<!-- TODO: used for debugging, remove once fixed -->
```
$ git config list --local
```

```
$ rad .
rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

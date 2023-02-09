To delete a repository from local storage, we use the `rad rm` command.
First let's look at what we have locally:

```
$ rad ls
heartwood rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji f2de534 Radicle Heartwood Protocol & Stack
```

Now let's delete the `heartwood` project:

```
$ rad rm rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-confirm
** Warning: Failed to untrack repository: failed to connect to node: No such file or directory (os error 2)
** Warning: Make sure to untrack this repository when your node is running
ok Successfully removed project rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from storage
```

We can check our repositories again to see if it was deleted:

```
$ rad ls
```

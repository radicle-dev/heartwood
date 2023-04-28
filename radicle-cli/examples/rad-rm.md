To delete a repository from local storage, we use the `rad rm` command.
First let's look at what we have locally:

```
$ rad ls
heartwood rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji f2de534 Radicle Heartwood Protocol & Stack
```

Now let's delete the `heartwood` project:

```
$ rad rm rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --no-confirm
✓ Untracked rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Successfully removed 'rad' remote
✓ Successfully removed rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from storage
```

We can check our repositories again to see if it was deleted:

```
$ rad ls
```

Attempting to remove a repository that doesn't exist gives us an error message:

```
$ rad rm rad:z2Jk1mNqyX7AjT4K83jJW9vQoHn4f
✗ Remove failed: repository rad:z2Jk1mNqyX7AjT4K83jJW9vQoHn4f was not found
```

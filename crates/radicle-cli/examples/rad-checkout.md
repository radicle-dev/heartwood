With the `rad checkout` command, you can create a new working copy from an
existing project.

```
$ rad checkout rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Repository checkout successful under ./heartwood
```

Let's have a look at what the command did. Navigate to the working copy:

```
$ cd heartwood
```

Check the README:
```
$ cat README
Hello World!
```

Check the repository status:

```
$ git status
On branch master
Your branch is up to date with 'rad/master'.

nothing to commit, working tree clean
```

Check the remote configuration:

```
$ git remote --verbose
rad	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji (fetch)
rad	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (push)
```

List the branches:

```
$ git branch --all
* master
  remotes/rad/master
```

List the references:

```
$ git show-ref
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927 refs/heads/master
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927 refs/remotes/rad/master
```

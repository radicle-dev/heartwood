With the `rad checkout` command, you can create a new working copy from an
existing project.

```
$ rad checkout rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji

Initializing local checkout for 🌱 rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji (heartwood)

ok Performing checkout...

🌱 Project checkout successful under ./heartwood

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
rad	rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (fetch)
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
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/heads/master
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354 refs/remotes/rad/master
```

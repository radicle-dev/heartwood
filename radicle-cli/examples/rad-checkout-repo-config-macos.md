```
$ cd heartwood
```

View the repository configuration:

```
$ cat .git/config
[core]
	bare = false
	repositoryformatversion = 0
	filemode = true
	ignorecase = true
	precomposeunicode = true
	logallrefupdates = true
[remote "rad"]
	url = rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
	fetch = +refs/heads/*:refs/remotes/rad/*
	pushurl = rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
[branch "master"]
	remote = rad
	merge = refs/heads/master
```

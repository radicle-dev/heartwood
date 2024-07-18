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
	logallrefupdates = true
[push]
	default = upstream
[remote "rad"]
	url = rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
	fetch = +refs/heads/*:refs/remotes/rad/*
	fetch = +refs/tags/*:refs/remotes/rad/tags/*
	pushurl = rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
[branch "master"]
	remote = rad
	merge = refs/heads/master
```

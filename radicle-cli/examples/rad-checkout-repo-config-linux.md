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
	url = rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
	fetch = +refs/heads/*:refs/remotes/rad/*
	fetch = +refs/tags/*:refs/remotes/rad/tags/*
	pushurl = rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
[branch "master"]
	remote = rad
	merge = refs/heads/master
```

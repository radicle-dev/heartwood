Alice creates an annotated tag and pushed to her `rad` remote:

``` ~alice
$ touch LICENSE
$ git add LICENSE
$ git commit -am "Add LICENSE"
[master 62d19fd] Add LICENSE
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
$ git tag v1.0 -a -m "Release v1.0"
```

``` ~alice (stderr)
$ git push rad v1.0
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new tag]         v1.0 -> v1.0
```

Since the `rad` remote is setup to push `tags` using the refspec:

~~~
fetch = +refs/tags/*:refs/remotes/<name>/tags/*
~~~

there's no need to use the `--tags` flag. In fact, we avoid fetching tags into
the global `tags` namespace to keep tags for each remote separate. This is
achieved by also adding the following option to the remote configuration:

~~~
tagOpt = --no-tags
~~~

Bob fetches the tag from Alice, by adding her as a remote:

``` ~bob
$ cd heartwood
$ rad remote add z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --name alice
✓ Follow policy updated for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Remote alice added
✓ Remote-tracking branch alice/master created for z6MknSL…StBU8Vi
```

Bob is able to fetch Alice's tag into his working copy, and they're fetched
under the `alice` remote:

``` ~bob (stderr)
$ git fetch alice
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new tag]         v1.0       -> alice/tags/v1.0
```

Alice forcefully creates a new version of the tag (let's say she made
a mistake):

``` ~alice
$ git commit --allow-empty -m "Release: v1.0"
[master 8260c04] Release: v1.0
$ git tag v1.0 -f -a -m "Release v1.0"
Updated tag 'v1.0' (was be18ed6)
```

``` ~alice (stderr)
$ git push rad v1.0 -f
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 + be18ed6...9dbdebc v1.0 -> v1.0 (forced update)
```

We ensure that Bob is still able to fetch from Alice and get the new
update of the tag:

``` ~bob
$ rad sync -f
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
```

``` ~bob (stderr)
$ git fetch alice -f
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   62d19fd..9dbdebc  v1.0       -> alice/tags/v1.0
```

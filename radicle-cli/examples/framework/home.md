```
$ mkdir bin
$ cd bin
$ touch file.bin
```

``` ~bob
$ rad self --did
did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ pwd
[..]/home/bob/.radicle
$ mkdir src
$ cd src
$ pwd
[..]/home/bob/.radicle/src
```

``` ~alice
$ rad self --did
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ pwd
[..]/home/alice/.radicle
```

``` ~bob
$ pwd
[..]/home/bob/.radicle/src
```

```
$ ls
file.bin
```

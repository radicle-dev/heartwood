In this example, we will show how we can make other references become canonical.
To illustrate, we will use Git tags as an example. The storage of the repository
should look something like this by the end of the example:

~~~
storage/z6cFWeWpnZNHh9rUW8phgA3b5yGt/refs
├── heads
│   └── main
├── namespaces
│   ├── z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
│   │   └── refs
│   │       ├── cobs
│   │       │   └── xyz.radicle.id
│   │       │       └── 865c48204bd7bb7f088b8db90ffdccb48cfa0a50
│   │       ├── heads
│   │       │   └── master
│   │       ├── tags
│   │       │   ├── v1.0-hotfix
│   │       │   └── v1.0
│   │       └── rad
│   │           ├── id
│   │           └── sigrefs
│   └── z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
│       └── refs
│           ├── heads
│           │   └── master
│           ├── tags
│           │   ├── v1.0-hotfix
│           │   └── v1.0
│           └── rad
│               ├── id
│               └── sigrefs
├── rad
│   └── id
└── tags
    ├── v1.0-hotfix
    └── v1.0
~~~

Noting that there are tags under `refs/tags` now.

To start, Alice will add a new payload to the repository identity. The identifier
for this payload is `xyz.radicle.project.canonicalReferences`. It contains a
single field with the key `rules`, and the value for this key is an array of
rules. In this case, we will have two rules: one for `refs/tags/*` and one for
`refs/tags/qa/*` (see RIP-0004 for more information on the rules).

``` ~alice
$ rad cref add refs/tags/* --threshold 1 --title "Add rule for refs/tags/*"
✓ Rule for refs/tags/* has been added
✓ Identity revision f4eda597611ec04ccf8bb3f18ddede4801a8441a created
$ rad cref add refs/tags/qa/* --threshold 1 --title "Add rule for refs/tags/qa/*"
✓ Rule for refs/tags/qa/* has been added
✓ Identity revision 4b02f0d2040de5970eab9b1889321387f379ef5c created
```

Now, Alice will create a tag and push it:

``` ~alice
$ git tag v1.0-hotfix
```

``` ~alice (stderr)
$ git push rad --tags
✓ Canonical head for refs/tags/v1.0-hotfix updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new tag]         v1.0-hotfix -> v1.0-hotfix
```

Notice that the output included a message about a canonical reference being
updated:

~~~
✓ Canonical reference refs/tags/v1.0-hotfix updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
~~~

On the other side, Bob performs a fetch and now has the tags locally:

``` ~bob (stderr)
$ cd heartwood
$ git fetch rad
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
 * [new tag]         v1.0-hotfix -> rad/tags/v1.0-hotfix
 * [new tag]         v1.0-hotfix -> v1.0-hotfix
```

In the next portion of this example, we want to show that using a `threshold` of
`2` requires both delegates. To do this, Bob creates a `master` reference, Alice
adds him as a remote, and adds him to the identity delegates, as well as setting
the `threshold` to `2` for the `refs/tags/*` rule:

``` ~bob
$ rad remote add z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --name alice
✓ Follow policy updated for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (alice)
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Remote alice added
✓ Remote-tracking branch alice/master created for z6MknSL…StBU8Vi
$ git push rad master
```

``` ~alice
$ rad remote add z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --name bob
✓ Follow policy updated for z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (bob)
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6Mkt67…v4N1tRk@[..]..
✓ Remote bob added
✓ Remote-tracking branch bob/master created for z6Mkt67…v4N1tRk
$ rad id update --title "Add Bob" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
f0fd3b0c65ec38e52d578df94f96a0f57ac27d65
$ rad cref edit refs/tags/* --threshold 2 --title "Change threshold for refs/tags to 2"
✓ Rule for refs/tags/* has been modified
✓ Identity revision ef105be657f3f112d0a3cfaafdf7362bc2df786a created
```

``` ~bob
$ rad sync -f
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
$ rad id accept ef105be657f3f112d0a3cfaafdf7362bc2df786a -q
```

When Bob creates a new tag and pushes it, we see that there's a warning that
no quorum was found for the new tag:

``` ~bob (stderr)
$ git tag v2.0
$ git push rad --tags
warn: could not determine tip for canonical reference 'refs/tags/v2.0', no commit with at least 2 vote(s) found (threshold not met)
warn: it is recommended to find a commit to agree upon
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new tag]         v1.0-hotfix -> v1.0-hotfix
 * [new tag]         v2.0 -> v2.0
```

Alice can then fetch and checkout the new tag, create one on her side, and push
it:

``` ~alice (stderr)
$ git fetch bob
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new tag]         v1.0-hotfix -> bob/tags/v1.0-hotfix
 * [new tag]         v2.0        -> bob/tags/v2.0
```

``` ~alice
$ git checkout bob/tags/v2.0
$ git tag v2.0
```

``` ~alice (stderr)
$ git push rad --tags
✓ Canonical head for refs/tags/v2.0 updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new tag]         v2.0 -> v2.0
```

Now that Bob has also pushed this tag, we can see that the tag was made
canonical.

For the final portion of the example, we will show that both delegates aren't
required for pushing tags that match the rule `refs/tags/qa/*`. To show this,
Bob will create a tag and push it, and we should see that the canonical
reference is created:

``` ~bob (stderr)
$ git tag qa/v2.1
$ git push rad --tags
✓ Canonical head for refs/tags/qa/v2.1 updated to f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new tag]         qa/v2.1 -> qa/v2.1
```

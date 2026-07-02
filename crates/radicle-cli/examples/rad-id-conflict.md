First let's add Bob as a delegate, and sync the changes to Bob:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --threshold 2 -q
0ca42d376bd566631083c8913cf86bec722da392
```
``` ~bob
$ cd heartwood
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

One thing that can happen is that two delegates propose a revision at the same
time:

``` ~alice
$ rad id update --title "Edit project name" --description "" --payload "xyz.radicle.project" "name" '"heart"' -q
12d7300d1bbba84e4e5760c8c61999bf5fefb81a
```
``` ~bob
$ rad id update --title "Edit project name" --description "" --payload "xyz.radicle.project" "name" '"wood"' -q
89b2623e7f2ddf5748661b15b9975ab0b4ee17ab
```

When Alice syncs with Bob, she notices the problem: there are two active
revisions.

``` ~alice
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title               Author                                                      Status     Created   Parent  │
├────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   89b2623   Edit project name   bob      z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk   active     now       0ca42d3 │
│ ●   12d7300   Edit project name   alice    (you)                                              active     now       0ca42d3 │
│ ●   0ca42d3   Add Bob             alice    (you)                                              accepted   now       0656c21 │
│ ●   0656c21   Initial revision    alice    (you)                                              accepted   now       none    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

This isn't a problem as long as we don't try to accept both. Alice decides to
go with Bob's proposal, so she redacts her own first and then accepts his:

``` ~alice
$ rad id redact 12d7300 -q
$ rad id accept 89b2623 -q
$ rad id list
╭────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title               Author                                                      Status     Created   Parent  │
├────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   89b2623   Edit project name   bob      z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk   accepted   now       0ca42d3 │
│ ●   0ca42d3   Add Bob             alice    (you)                                              accepted   now       0656c21 │
│ ●   0656c21   Initial revision    alice    (you)                                              accepted   now       none    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Alice's revision was redacted and is no longer visible. Bob syncs and sees
that the conflict is resolved:

``` ~bob
$ rad sync --fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```
``` ~bob (fail)
$ rad id show 12d7300
✗ Error: revision `12d7300d1bbba84e4e5760c8c61999bf5fefb81a` not found
```

# Showing a default-target patch when canonical head is unresolved

Alice and Bob are delegates with threshold `2`. Bob's fork was synced
earlier, but now Bob's refs are unavailable to Alice and the canonical
default branch cannot be computed.

First, create a patch against the default branch:

``` ~alice (stderr)
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
$ git commit -a -m "Add things" -q --allow-empty
```

``` ~alice (stderr)
$ git push -o no-sync -o patch.message="Add things #1" -o patch.message="See commits for details." rad HEAD:refs/patches
✓ Patch 6035d2f582afbe01ff23ea87528ae523d76875b6 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Then add Bob as a delegate with threshold `2` (Bob already has a fork,
so the identity update succeeds):

``` ~alice
$ rad id update --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
[..]
```

Now simulate the canonical default branch becoming unresolved: Bob's
refs are gone and the canonical `master` ref is missing:

``` ~alice
$ git --git-dir="$RAD_HOME/storage/z42hL2jL4XNk6K8oHQaSWfMgCL7ji" update-ref -d refs/namespaces/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/refs/rad/sigrefs
$ git --git-dir="$RAD_HOME/storage/z42hL2jL4XNk6K8oHQaSWfMgCL7ji" update-ref -d refs/namespaces/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/refs/heads/master
$ git --git-dir="$RAD_HOME/storage/z42hL2jL4XNk6K8oHQaSWfMgCL7ji" update-ref -d refs/heads/master
```

`rad patch show` degrades the commits row instead of failing, because
resolving the default target would require the canonical head, which
cannot be computed:

``` ~alice
$ rad patch show 6035d2f582afbe01ff23ea87528ae523d76875b6
╭──────────────────────────────────────────────────────────╮
│ Title     Add things #1                                  │
│ Patch     6035d2f582afbe01ff23ea87528ae523d76875b6       │
│ Author    alice (you)                                    │
│ Head      42d894a83c9c356552a57af09ccdbd5587a99045       │
│ Base      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354       │
│ Target    master                                         │
│ Branches  feature/1                                      │
│ Commits   unknown                                        │
│ Status    open                                           │
│                                                          │
│ See commits for details.                                 │
├──────────────────────────────────────────────────────────┤
│ 42d894a Add things                                       │
├──────────────────────────────────────────────────────────┤
│ ● Revision 6035d2f @ f2de534..42d894a by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

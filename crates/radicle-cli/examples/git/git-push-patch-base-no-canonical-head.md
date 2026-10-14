# Opening a patch with an explicit base while the canonical head is unresolved

Normally, opening a patch with an explicit base should not require the
canonical head to be computed. This test sets up a two-delegate repository
with threshold 2, syncs Bob, then removes Bob's refs from Alice's storage.
The canonical head is then unresolved, but opening a patch with an explicit
base should still succeed.

``` ~alice
$ rad id update --title "Add Bob" --description "" --threshold 2 --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-confirm -q
[..]
```

Remove Bob's signed refs and branch from Alice's storage:

``` ~alice
$ git --git-dir="$RAD_HOME/storage/z42hL2jL4XNk6K8oHQaSWfMgCL7ji" update-ref -d refs/namespaces/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/refs/rad/sigrefs
$ git --git-dir="$RAD_HOME/storage/z42hL2jL4XNk6K8oHQaSWfMgCL7ji" update-ref -d refs/namespaces/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk/refs/heads/master
```

Force the storage repository's `HEAD` to point at a branch that does not exist
so that `Repository::head` falls back to the canonical head.

``` ~alice
$ git --git-dir="$RAD_HOME/storage/z42hL2jL4XNk6K8oHQaSWfMgCL7ji" symbolic-ref HEAD refs/heads/accepted
```

``` ~alice (stderr)
$ git checkout -b feature/1
Switched to a new branch 'feature/1'
```

``` ~alice
$ git commit --allow-empty -m "Add new feature"
[feature/1 [..]] Add new feature
```

``` ~alice (stderr)
$ git push -o no-sync -o patch.message="Add new feature" -o patch.base="f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354" rad HEAD:refs/patches
✓ Patch [..] opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

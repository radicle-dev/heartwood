Let's look at how patch updates work.

Alice creates a repository and Bob clones it.

``` ~alice
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --public

Initializing public Radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK
You can show it any time by running `rad .` from this directory.

✓ Repository successfully announced to the network.

Your repository has been announced to the network and is now discoverable by peers.
You can check for any nodes that have replicated your repository by running `rad sync status`.

To push changes, run `git push`.
```

``` ~bob
$ rad clone rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK
✓ Seeding policy updated for rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK with scope 'followed'
Fetching rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ radicle heartwood protocol & stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
```

We fork the repository by pushing to `master`, and wait for Alice to sync
our fork:

``` ~bob (stderr)
$ cd heartwood
$ git push rad master
✓ Synced with 1 seed(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      master -> master
```

Bob then opens a patch.

``` ~bob (stderr)
$ git checkout -b bob/feature -q
$ git commit --allow-empty -m "Bob's commit #1" -q
$ git push rad -o sync -o patch.message="Bob's patch" HEAD:refs/patches
✓ Patch 5d5687e3d46f81a1e0283b252bd6f206914884f8 opened
✓ Synced with 1 seed(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```
``` ~bob
$ git status --short --branch
## bob/feature...rad/patches/5d5687e3d46f81a1e0283b252bd6f206914884f8
```

Alice checks it out.

``` ~alice
$ rad patch checkout 5d5687e3d46f81a1e0283b252bd6f206914884f8
✓ Switched to branch patch/5d5687e at revision 5d5687e
✓ Branch patch/5d5687e setup to track rad/patches/5d5687e3d46f81a1e0283b252bd6f206914884f8
$ git show
commit dd7b34d00776fcc562fe60e4d54f4d0021b919ef
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Bob's commit #1
```

Bob then updates the patch.

``` ~bob (stderr)
$ git commit --allow-empty -m "Bob's commit #2" -q
$ git push rad -o sync -o patch.message="Updated."
✓ Patch 5d5687e updated to revision 87c9c1e800ae56a8f0974700ca2bd9fec8edef83
To compare against your previous revision 5d5687e, run:

   git range-diff 4c66f0e[..] dd7b34d[..] 954a173[..]

✓ Synced with 1 seed(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   dd7b34d..954a173  bob/feature -> patches/5d5687e3d46f81a1e0283b252bd6f206914884f8
```

Alice pulls the update.

``` ~alice
$ rad patch show 5d5687e
╭──────────────────────────────────────────────────────────────────╮
│ Title    Bob's patch                                             │
│ Patch    5d5687e3d46f81a1e0283b252bd6f206914884f8                │
│ Author   bob z6Mkt67…v4N1tRk                                     │
│ Head     954a173c97031553ca7388bb9ab7066862c49f18                │
│ Base     [..                                          ]          │
│ Target   master                                                  │
│ Commits  ahead 2, behind 0                                       │
│ Status   open                                                    │
├──────────────────────────────────────────────────────────────────┤
│ 954a173 Bob's commit #2                                          │
│ dd7b34d Bob's commit #1                                          │
├──────────────────────────────────────────────────────────────────┤
│ ● Revision 5d5687e @ [..   ]..dd7b34d by bob z6Mkt67…v4N1tRk now │
│ ↑ Revision 87c9c1e @ [..   ]..954a173 by bob z6Mkt67…v4N1tRk now │
╰──────────────────────────────────────────────────────────────────╯
$ git ls-remote rad
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	HEAD
4c66f0e93e6d341fa0ad45a2b4a2e8cb0fed5927	refs/heads/master
954a173c97031553ca7388bb9ab7066862c49f18	refs/heads/patches/5d5687e3d46f81a1e0283b252bd6f206914884f8
```
``` ~alice
$ git fetch rad
$ git status --short --branch
## patch/5d5687e...rad/patches/5d5687e3d46f81a1e0283b252bd6f206914884f8 [behind 1]
```
``` ~alice
$ git pull
Updating dd7b34d..954a173
Fast-forward
```
``` ~alice
$ git show
commit 954a173c97031553ca7388bb9ab7066862c49f18
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Bob's commit #2
```

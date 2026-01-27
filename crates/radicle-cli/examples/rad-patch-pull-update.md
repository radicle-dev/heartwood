Let's look at how patch updates work.

Alice creates a repository and Bob clones it.

``` ~alice
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --public

Initializing public radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK.
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
✓ Patch 55b9721ed7f6bfec38f43729e9b6631c5dc812fb opened
✓ Synced with 1 seed(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```
``` ~bob
$ git status --short --branch
## bob/feature...rad/patches/55b9721ed7f6bfec38f43729e9b6631c5dc812fb
```

Alice checks it out.

``` ~alice
$ rad patch checkout 55b9721ed7f6bfec38f43729e9b6631c5dc812fb
✓ Switched to branch patch/55b9721 at revision 55b9721
✓ Branch patch/55b9721 setup to track rad/patches/55b9721ed7f6bfec38f43729e9b6631c5dc812fb
$ git show
commit bdcdb30b3c0f513620dd0f1c24ff8f4f71de956b
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Bob's commit #1
```

Bob then updates the patch.

``` ~bob (stderr)
$ git commit --allow-empty -m "Bob's commit #2" -q
$ git push rad -o sync -o patch.message="Updated."
✓ Patch 55b9721 updated to revision f91e056da05b2d9a58af1160c76245bc3debf7a8
To compare against your previous revision 55b9721, run:

   git range-diff f2de534[..] bdcdb30[..] cad2666[..]

✓ Synced with 1 seed(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   bdcdb30..cad2666  bob/feature -> patches/55b9721ed7f6bfec38f43729e9b6631c5dc812fb
```

Alice pulls the update.

``` ~alice
$ rad patch show 55b9721
╭─────────────────────────────────────────────────────────╮
│ Title    Bob's patch                                    │
│ Patch    55b9721ed7f6bfec38f43729e9b6631c5dc812fb       │
│ Author   bob z6Mkt67…v4N1tRk                            │
│ Head     cad2666a8a2250e4dee175ed5044be2c251ff08b       │
│ Base     [..                                          ] │
│ Commits  ahead 2, behind 0                              │
│ Status   open                                           │
├─────────────────────────────────────────────────────────┤
│ cad2666 Bob's commit #2                                 │
│ bdcdb30 Bob's commit #1                                 │
├─────────────────────────────────────────────────────────┤
│ ● Revision 55b9721 @ bdcdb30 by bob z6Mkt67…v4N1tRk now │
│ ↑ Revision f91e056 @ cad2666 by bob z6Mkt67…v4N1tRk now │
╰─────────────────────────────────────────────────────────╯
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	HEAD
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
cad2666a8a2250e4dee175ed5044be2c251ff08b	refs/heads/patches/55b9721ed7f6bfec38f43729e9b6631c5dc812fb
```
``` ~alice
$ git fetch rad
$ git status --short --branch
## patch/55b9721...rad/patches/55b9721ed7f6bfec38f43729e9b6631c5dc812fb [behind 1]
```
``` ~alice
$ git pull
Updating bdcdb30..cad2666
Fast-forward
```
``` ~alice
$ git show
commit cad2666a8a2250e4dee175ed5044be2c251ff08b
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Bob's commit #2
```

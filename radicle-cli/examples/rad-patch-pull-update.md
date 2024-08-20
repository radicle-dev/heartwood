Let's look at how patch updates work.

Alice creates a repository and Bob clones it.

``` ~alice
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --public

Initializing public radicle 👾 repository in [..]

✓ Repository heartwood created.

Your Repository ID (RID) is rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg.
You can show it any time by running `rad .` from this directory.

✓ Repository successfully announced to the network.

Your repository has been announced to the network and is now discoverable by peers.
You can check for any nodes that have replicated your repository by running `rad sync status`.

To push changes, run `git push`.
```

``` ~bob
$ rad clone rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg
✓ Seeding policy updated for rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg with scope 'all'
✓ Fetching rad:z3Lr338KCqbiwiLSh9DQZxTiLQUHg from z6MknSL…StBU8Vi..
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
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
✓ Synced with 1 node(s)
To rad://z3Lr338KCqbiwiLSh9DQZxTiLQUHg/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      master -> master
```

Bob then opens a patch.

``` ~bob (stderr)
$ git checkout -b bob/feature -q
$ git commit --allow-empty -m "Bob's commit #1" -q
$ git push rad -o sync -o patch.message="Bob's patch" HEAD:refs/patches
✓ Patch f4563fc729c1361df8040cc26fd7bc7cf51a81fc opened
✓ Synced with 1 node(s)
To rad://z3Lr338KCqbiwiLSh9DQZxTiLQUHg/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```
``` ~bob
$ git status --short --branch
## bob/feature...rad/patches/f4563fc729c1361df8040cc26fd7bc7cf51a81fc
```

Alice checks it out.

``` ~alice
$ rad patch checkout f4563fc729c1361df8040cc26fd7bc7cf51a81fc
✓ Switched to branch patch/f4563fc at revision f4563fc
✓ Branch patch/f4563fc setup to track rad/patches/f4563fc729c1361df8040cc26fd7bc7cf51a81fc
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
✓ Patch f4563fc updated to revision ae9105fdd4c9d91ea920f8a651e088a3bbdab830
✓ Synced with 1 node(s)
To rad://z3Lr338KCqbiwiLSh9DQZxTiLQUHg/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   bdcdb30..cad2666  bob/feature -> patches/f4563fc729c1361df8040cc26fd7bc7cf51a81fc
```

Alice pulls the update.

``` ~alice
$ rad patch show f4563fc
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Bob's patch                                                │
│ Patch    f4563fc729c1361df8040cc26fd7bc7cf51a81fc                   │
│ Author   bob z6Mkt67…v4N1tRk                                        │
│ Head     cad2666a8a2250e4dee175ed5044be2c251ff08b                   │
│ Commits  ahead 2, behind 0                                          │
│ Status   open                                                       │
├─────────────────────────────────────────────────────────────────────┤
│ cad2666 Bob's commit #2                                             │
│ bdcdb30 Bob's commit #1                                             │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by bob z6Mkt67…v4N1tRk (bdcdb30) now                       │
│ ↑ updated to ae9105fdd4c9d91ea920f8a651e088a3bbdab830 (cad2666) now │
╰─────────────────────────────────────────────────────────────────────╯
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
cad2666a8a2250e4dee175ed5044be2c251ff08b	refs/heads/patches/f4563fc729c1361df8040cc26fd7bc7cf51a81fc
```
``` ~alice
$ git fetch rad
$ git status --short --branch
## patch/f4563fc...rad/patches/f4563fc729c1361df8040cc26fd7bc7cf51a81fc [behind 1]
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

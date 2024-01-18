Let's look at how patch updates work.

Alice creates a repository and Bob clones it.

``` ~alice
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --public

Initializing public radicle 👾 repository in .

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
✓ Seeding policy updated for rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK with scope 'all'
✓ Fetching rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK from z6MknSL…StBU8Vi..
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
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      master -> master
```

Bob then opens a patch.

``` ~bob (stderr)
$ git checkout -b bob/feature -q
$ git commit --allow-empty -m "Bob's commit #1" -q
$ git push rad -o sync -o patch.message="Bob's patch" HEAD:refs/patches
✓ Patch d83a9a0e808dea392a10c0a62246484a18842039 opened
✓ Synced with 1 node(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```
``` ~bob
$ git status --short --branch
## bob/feature...rad/patches/d83a9a0e808dea392a10c0a62246484a18842039
```

Alice checks it out.

``` ~alice
$ rad patch checkout d83a9a0
✓ Switched to branch patch/d83a9a0
✓ Branch patch/d83a9a0 setup to track rad/patches/d83a9a0e808dea392a10c0a62246484a18842039
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
✓ Patch d83a9a0 updated to revision 37d68f4660541fc4fcbba5b7f81cc8f79b6cb05c
✓ Synced with 1 node(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   bdcdb30..cad2666  bob/feature -> patches/d83a9a0e808dea392a10c0a62246484a18842039
```

Alice pulls the update.

``` ~alice
$ rad patch show d83a9a0
╭─────────────────────────────────────────────────────────────────────╮
│ Title    Bob's patch                                                │
│ Patch    d83a9a0e808dea392a10c0a62246484a18842039                   │
│ Author   bob z6Mkt67…v4N1tRk                                        │
│ Head     cad2666a8a2250e4dee175ed5044be2c251ff08b                   │
│ Commits  ahead 2, behind 0                                          │
│ Status   open                                                       │
├─────────────────────────────────────────────────────────────────────┤
│ cad2666 Bob's commit #2                                             │
│ bdcdb30 Bob's commit #1                                             │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by bob z6Mkt67…v4N1tRk (bdcdb30) now                       │
│ ↑ updated to 37d68f4660541fc4fcbba5b7f81cc8f79b6cb05c (cad2666) now │
╰─────────────────────────────────────────────────────────────────────╯
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
cad2666a8a2250e4dee175ed5044be2c251ff08b	refs/heads/patches/d83a9a0e808dea392a10c0a62246484a18842039
```
``` ~alice
$ git fetch rad
$ git status --short --branch
## patch/d83a9a0...rad/patches/d83a9a0e808dea392a10c0a62246484a18842039 [behind 1]
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

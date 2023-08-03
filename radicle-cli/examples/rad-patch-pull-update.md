Let's look at how patch updates work.

Alice creates a project and Bob clones it.

``` ~alice
$ rad init --name heartwood --description "radicle heartwood protocol & stack" --no-confirm --announce

Initializing radicle 👾 project in .

✓ Project heartwood created
✓ Syncing inventory..
✓ Announcing inventory..

Your project's Repository ID (RID) is rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK.
You can show it any time by running `rad .`
```

``` ~bob
$ rad clone rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK
✓ Tracking relationship established for rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK with scope 'all'
✓ Fetching rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK from z6MknSL…StBU8Vi..
✓ Forking under z6Mkt67…v4N1tRk..
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSL…StBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
```

We wait for Alice to sync our fork.

``` ~bob
$ rad node events -n 1 --timeout 1
{"type":"refsSynced","remote":"z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi","rid":"rad:zhbMU4DUXrzB8xT6qAJh6yZ7bFMK"}
```

Bob then opens a patch.

``` ~bob (stderr)
$ cd heartwood
$ git checkout -b bob/feature -q
$ git commit --allow-empty -m "Bob's commit #1" -q
$ git push rad -o sync -o patch.message="Bob's patch" HEAD:refs/patches
✓ Patch 627477fdb46b9aaf3f0677c415b569cd21227b76 opened
✓ Synced with 1 node(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```
``` ~bob
$ git status --short --branch
## bob/feature...rad/patches/627477fdb46b9aaf3f0677c415b569cd21227b76
```

Alice checks it out.

``` ~alice
$ rad patch checkout 627477f
✓ Switched to branch patch/627477f
✓ Branch patch/627477f setup to track rad/patches/627477fdb46b9aaf3f0677c415b569cd21227b76
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
✓ Patch 627477f updated to c4114446af35501300c68571cfb07a6f5c7e1eef
✓ Synced with 1 node(s)
To rad://zhbMU4DUXrzB8xT6qAJh6yZ7bFMK/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   bdcdb30..cad2666  bob/feature -> patches/627477fdb46b9aaf3f0677c415b569cd21227b76
```

Alice pulls the update.

``` ~alice
$ rad patch show 627477f
╭──────────────────────────────────────────────────────────────────────────────╮
│ Title    Bob's patch                                                         │
│ Patch    627477fdb46b9aaf3f0677c415b569cd21227b76                            │
│ Author   did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk            │
│ Head     cad2666a8a2250e4dee175ed5044be2c251ff08b                            │
│ Commits  ahead 2, behind 0                                                   │
│ Status   open                                                                │
├──────────────────────────────────────────────────────────────────────────────┤
│ cad2666 Bob's commit #2                                                      │
│ bdcdb30 Bob's commit #1                                                      │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● opened by bob (z6Mkt67…v4N1tRk) [   ...    ]                               │
│ ↑ updated to c4114446af35501300c68571cfb07a6f5c7e1eef (cad2666) [   ...    ] │
╰──────────────────────────────────────────────────────────────────────────────╯
$ git ls-remote rad
f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354	refs/heads/master
cad2666a8a2250e4dee175ed5044be2c251ff08b	refs/heads/patches/627477fdb46b9aaf3f0677c415b569cd21227b76
```
``` ~alice
$ git fetch rad
$ git status --short --branch
## patch/627477f...rad/patches/627477fdb46b9aaf3f0677c415b569cd21227b76 [behind 1]
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

In some rare scenarios, a delegate will merge a patch without having first
pushed to their default branch.

Here we start off with Alice adding Bob as a delegate:

``` ~alice
$ rad id -q update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
7be665f9fccba97abb21b2fa85a6fd3181c72858
```

Bob clones the repository so that he can start working on it with Alice:

``` ~bob
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Creating checkout in ./heartwood..
✓ Remote alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi added
✓ Remote-tracking branch alice@z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master created for z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ cd heartwood
```

Alice then goes ahead and prepares a patch:

``` ~alice
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

``` ~alice (stderr)
$ git push rad -o no-sync -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch df868fc7985fa7217aaef8423d73a077868c02b3 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Bob, as a delegate decides to review the patch and deems it worthy to merge, so
first he syncs the repository:

``` ~bob
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

He then checks out the patch and merges the changes in:

``` ~bob
$ rad patch checkout df868fc
✓ Switched to branch patch/df868fc at revision df868fc
✓ Branch patch/df868fc setup to track rad/patches/df868fc7985fa7217aaef8423d73a077868c02b3
$ git checkout master
Your branch is up to date with 'rad/master'.
$ git merge patch/df868fc
Updating f2de534..3e674d1
Fast-forward
 REQUIREMENTS | 0
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

Bob has not pushed a `master` yet, and we can confirm this by listing the
references in storage:

``` ~bob
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── 0656c217f917c3e06234771e9ecae53aba5e173e
    │   └── xyz.radicle.patch
    │       └── df868fc7985fa7217aaef8423d73a077868c02b3
    ├── heads
    │   ├── master
    │   └── patches
    │       └── df868fc7985fa7217aaef8423d73a077868c02b3
    └── rad
        ├── id
        ├── root
        └── sigrefs
```

Notice that there are no references for `z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk`, Bob's NID.

Now that the changes are merged, he can push and have the patch merged:

``` ~bob (stderr)
$ git push rad master
✓ Patch df868fc7985fa7217aaef8423d73a077868c02b3 merged
✓ Canonical reference refs/heads/master updated to target commit 3e674d1a1df90807e934f9ae5da2591dd6848a33
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new branch]      master -> master
```

We can check the references again to see Bob's namespace now appear with `master`:

``` ~bob
$ rad inspect --refs
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
└── refs
    ├── cobs
    │   ├── xyz.radicle.id
    │   │   └── 0656c217f917c3e06234771e9ecae53aba5e173e
    │   └── xyz.radicle.patch
    │       └── df868fc7985fa7217aaef8423d73a077868c02b3
    ├── heads
    │   ├── master
    │   └── patches
    │       └── df868fc7985fa7217aaef8423d73a077868c02b3
    └── rad
        ├── id
        ├── root
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── cobs
    │   └── xyz.radicle.patch
    │       └── df868fc7985fa7217aaef8423d73a077868c02b3
    ├── heads
    │   └── master
    └── rad
        ├── root
        └── sigrefs
```


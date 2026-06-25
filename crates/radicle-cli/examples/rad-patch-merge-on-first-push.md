In some rare scenarios, a delegate will merge a patch without having first
pushed to their default branch.

Here we start off with Alice adding Bob as a delegate:

``` ~alice
$ rad id update --title "Add Bob" --description "Add Bob as a delegate" --delegate did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk -q
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
[flux-capacitor-power 602ba44] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

``` ~alice (stderr)
$ git push rad -o no-sync -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 9e3196c453072852c68d3425be9000b5cb67ca3d opened
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
$ rad patch checkout 9e3196c
✓ Switched to branch patch/9e3196c at revision 9e3196c
✓ Branch patch/9e3196c setup to track rad/patches/9e3196c453072852c68d3425be9000b5cb67ca3d
$ git checkout master
Your branch is up to date with 'rad/master'.
$ git merge patch/9e3196c
Updating 4c66f0e..602ba44
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
    │       └── 9e3196c453072852c68d3425be9000b5cb67ca3d
    ├── heads
    │   ├── master
    │   └── patches
    │       └── 9e3196c453072852c68d3425be9000b5cb67ca3d
    └── rad
        ├── id
        ├── root
        └── sigrefs
```

Notice that there are no references for `z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk`, Bob's NID.

Now that the changes are merged, he can push and have the patch merged:

``` ~bob (stderr)
$ git push rad master
✓ Patch 9e3196c453072852c68d3425be9000b5cb67ca3d merged
✓ Canonical reference refs/heads/master updated to target commit 602ba4448210fba26633dc3f9ae3d4d9d20a1e84
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
    │       └── 9e3196c453072852c68d3425be9000b5cb67ca3d
    ├── heads
    │   ├── master
    │   └── patches
    │       └── 9e3196c453072852c68d3425be9000b5cb67ca3d
    └── rad
        ├── id
        ├── root
        └── sigrefs
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
└── refs
    ├── cobs
    │   └── xyz.radicle.patch
    │       └── 9e3196c453072852c68d3425be9000b5cb67ca3d
    ├── heads
    │   └── master
    └── rad
        ├── root
        └── sigrefs
```


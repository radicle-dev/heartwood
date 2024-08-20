A common workflow is to use `rad patch checkout` to view a
collaborator's changes. So, first off, we create a patch:

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
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch c90967c43719b916e0b5a8b5dafe353608f8a08a opened
✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

On the other end, Bob uses `rad patch checkout` to view the patch:

``` ~bob
$ cd heartwood
$ rad sync -f
✓ Fetching rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 from z6MknSL…StBU8Vi@[..]..
✓ Fetched repository from 1 seed(s)
$ rad patch checkout c90967c --name alice-init
✓ Switched to branch alice-init at revision c90967c
✓ Branch alice-init setup to track rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

Meanwhile, we may see some more changes that we need to make, so we
add a `README.md`:

``` ~alice
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` ~alice (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch c90967c updated to revision 594bb93b4ba836777c111053af7b61ff772afbc5
To compare against your previous revision c90967c, run:

   git range-diff f2de534[..] 3e674d1[..] 27857ec[..]

✓ Synced with 1 node(s)
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  flux-capacitor-power -> patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

Bob fetches these new changes and can see their branch is now behind:

``` ~bob (stderr)
$ git fetch rad
From rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2
   3e674d1..27857ec  patches/c90967c43719b916e0b5a8b5dafe353608f8a08a -> rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

``` ~bob
$ git status
On branch alice-init
Your branch is behind 'rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a' by 1 commit, and can be fast-forwarded.
  (use "git pull" to update your local branch)

nothing to commit, working tree clean
```

If Bob was to run `rad patch checkout` again, it would error.
This is because the branch already exists and `rad` does not want to
overwrite any changes. Bob can choose to use the `--force` (`-f`) flag to
ensure that they are looking at the latest changes:

``` ~bob (fail)
$ rad patch checkout c90967c --name alice-init
✗ Performing checkout... <canceled>
✗ Error: branch 'alice-init' already exists (use `--force` to overwrite)
```

``` ~bob
$ rad patch checkout c90967c -f --name alice-init
✓ Switched to branch alice-init at revision 594bb93
$ git status
On branch alice-init
Your branch is up to date with 'rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a'.

nothing to commit, working tree clean
```

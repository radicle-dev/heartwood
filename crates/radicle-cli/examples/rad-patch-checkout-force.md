A common workflow is to use `rad patch checkout` to view a
collaborator's changes. So, first off, we create a patch:

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
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 70a5938a2620ffad0cef086670112c65f69a3d48 opened
✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

On the other end, Bob uses `rad patch checkout` to view the patch:

``` ~bob
$ cd heartwood
$ rad sync -f
Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
🌱 Fetched from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ rad patch checkout 70a5938 --name alice-init
✓ Switched to branch alice-init at revision 70a5938
✓ Branch alice-init setup to track rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

Meanwhile, we may see some more changes that we need to make, so we
add a `README.md`:

``` ~alice
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 8083d4c] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

``` ~alice (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch 70a5938 updated to revision 052017158836193eb24c54b983614b062cb92bd0
To compare against your previous revision 70a5938, run:

   git range-diff 4c66f0e[..] 602ba44[..] 8083d4c[..]

✓ Synced with 1 seed(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   602ba44..8083d4c  flux-capacitor-power -> patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

Bob fetches these new changes and can see their branch is now behind:

``` ~bob (stderr)
$ git fetch rad
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
   602ba44..8083d4c  patches/70a5938a2620ffad0cef086670112c65f69a3d48 -> rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

``` ~bob
$ git status
On branch alice-init
Your branch is behind 'rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48' by 1 commit, and can be fast-forwarded.
  (use "git pull" to update your local branch)

nothing to commit, working tree clean
```

If Bob was to run `rad patch checkout` again, it would error.
This is because the branch already exists and `rad` does not want to
overwrite any changes. Bob can choose to use the `--force` (`-f`) flag to
ensure that they are looking at the latest changes:

``` ~bob (fail)
$ rad patch checkout 70a5938 --name alice-init
✗ Performing checkout… <canceled>
✗ Error: branch 'alice-init' already exists (use `--force` to overwrite)
```

``` ~bob
$ rad patch checkout 70a5938 -f --name alice-init
✓ Switched to branch alice-init at revision 0520171
$ git status
On branch alice-init
Your branch is up to date with 'rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48'.

nothing to commit, working tree clean
```

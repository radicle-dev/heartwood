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
✓ Patch 0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f opened
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

On the other end, Bob uses `rad patch checkout` to view the patch:

``` ~bob
$ cd heartwood
$ rad sync -f
✓ Fetching rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSL…StBU8Vi..
✓ Fetched repository from 1 seed(s)
$ rad patch checkout 0f3cd0b --name alice-init
✓ Switched to branch alice-init
✓ Branch alice-init setup to track rad/patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
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
✓ Patch 0f3cd0b updated to revision 6e6644973e3ecd0965b7bc5743f05a5fe1c7bff9
✓ Synced with 1 node(s)
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  flux-capacitor-power -> patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
```

Bob fetches these new changes and can see their branch is now behind:

``` ~bob (stderr)
$ git fetch rad
From rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji
   3e674d1..27857ec  patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f -> rad/patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
```

``` ~bob
$ git status
On branch alice-init
Your branch is behind 'rad/patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f' by 1 commit, and can be fast-forwarded.
  (use "git pull" to update your local branch)

nothing to commit, working tree clean
```

If Bob was to run `rad patch checkout` again, it would error.
This is because the branch already exists and `rad` does not want to
overwrite any changes. Bob can choose to use the `--force` (`-f`) flag to
ensure that they are looking at the latest changes:

``` ~bob (fail)
$ rad patch checkout 0f3cd0b --name alice-init
✗ Performing checkout... <canceled>
✗ Error: branch 'alice-init' already exists (use `--force` to overwrite)
```

``` ~bob
$ rad patch checkout 0f3cd0b -f --name alice-init
✓ Switched to branch alice-init
$ git status
On branch alice-init
Your branch is up to date with 'rad/patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f'.

nothing to commit, working tree clean
```

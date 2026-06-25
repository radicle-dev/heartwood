We may want to work on top of an existing patch and this where `rad
patch checkout` comes into play. So, first we will create a patch to
set up the workflow.

```
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
```

Here the instructions are added to the project's README for 1.21 gigawatts and
commit the changes to git.

```
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 602ba44] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

Once the code is ready, we open (or create) a patch with our changes for the project.

``` (stderr)
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 70a5938a2620ffad0cef086670112c65f69a3d48 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout 70a5938a2620ffad0cef086670112c65f69a3d48
✓ Switched to branch patch/70a5938 at revision 70a5938
✓ Branch patch/70a5938 setup to track rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

Note that `rad patch checkout` can be used to switch to the patch branch
as long as we haven't made changes to it.

```
$ git checkout master -q
$ rad patch checkout 70a5938
✓ Switched to branch patch/70a5938 at revision 70a5938
```

Now, let's add a README too!

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[patch/70a5938 8083d4c] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

We can now finish off the update:

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch 70a5938 updated to revision 052017158836193eb24c54b983614b062cb92bd0
To compare against your previous revision 70a5938, run:

   git range-diff 4c66f0e[..] 602ba44[..] 8083d4c[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   602ba44..8083d4c  patch/70a5938 -> patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

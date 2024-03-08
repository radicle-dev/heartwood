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
[flux-capacitor-power 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

Once the code is ready, we open (or create) a patch with our changes for the project.

``` (stderr)
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch aa45913e757cacd46972733bddee5472c78fa32a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout aa45913e757cacd46972733bddee5472c78fa32a
✓ Switched to branch patch/aa45913 at revision aa45913
✓ Branch patch/aa45913 setup to track rad/patches/aa45913e757cacd46972733bddee5472c78fa32a
```

Note that `rad patch checkout` can be used to switch to the patch branch
as long as we haven't made changes to it.

```
$ git checkout master -q
$ rad patch checkout aa45913
✓ Switched to branch patch/aa45913 at revision aa45913
```

Now, let's add a README too!

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[patch/aa45913 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

We can now finish off the update:

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch aa45913 updated to revision 3156bed9d64d4675d6cf56612d217fc5f4e8a53a
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  patch/aa45913 -> patches/aa45913e757cacd46972733bddee5472c78fa32a
```

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
✓ Patch 0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout 0f3cd0b
✓ Switched to branch patch/0f3cd0b
✓ Branch patch/0f3cd0b setup to track rad/patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
```

Note that `rad patch checkout` can be used to switch to the patch branch
as long as we haven't made changes to it.

```
$ git checkout master -q
$ rad patch checkout 0f3cd0b
✓ Switched to branch patch/0f3cd0b
```

Now, let's add a README too!

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[patch/0f3cd0b 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

We can now finish off the update:

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch 0f3cd0b updated to revision 6e6644973e3ecd0965b7bc5743f05a5fe1c7bff9
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  patch/0f3cd0b -> patches/0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
```

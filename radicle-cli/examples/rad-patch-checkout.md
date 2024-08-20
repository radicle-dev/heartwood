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
✓ Patch c90967c43719b916e0b5a8b5dafe353608f8a08a opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout c90967c43719b916e0b5a8b5dafe353608f8a08a
✓ Switched to branch patch/c90967c at revision c90967c
✓ Branch patch/c90967c setup to track rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

Note that `rad patch checkout` can be used to switch to the patch branch
as long as we haven't made changes to it.

```
$ git checkout master -q
$ rad patch checkout c90967c
✓ Switched to branch patch/c90967c at revision c90967c
```

Now, let's add a README too!

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[patch/c90967c 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

We can now finish off the update:

``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch c90967c updated to revision 594bb93b4ba836777c111053af7b61ff772afbc5
To compare against your previous revision c90967c, run:

   git range-diff f2de534[..] 3e674d1[..] 27857ec[..]

To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  patch/c90967c -> patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

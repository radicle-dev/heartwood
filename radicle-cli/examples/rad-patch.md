When contributing to another's project, it is common for the contribution to be
of many commits and involve a discussion with the project's maintainer.  This is supported
via Radicle's patches.

Here we give a brief overview for using patches in our hypothetical car
scenario.  It turns out instructions containing the power requirements were
missing from the project.

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
✓ Patch a8926643a8f6a65bc386b0131621994000485d4d opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author                  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  a892664  Define power requirements  z6MknSL…StBU8Vi  (you)  3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show a8926643a8f6a65bc386b0131621994000485d4d -p
╭────────────────────────────────────────────────────╮
│ Title     Define power requirements                │
│ Patch     a8926643a8f6a65bc386b0131621994000485d4d │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33 │
│ Branches  flux-capacitor-power                     │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
│                                                    │
│ See details.                                       │
├────────────────────────────────────────────────────┤
│ 3e674d1 Define power requirements                  │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) now              │
╰────────────────────────────────────────────────────╯

commit 3e674d1a1df90807e934f9ae5da2591dd6848a33
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Define power requirements

diff --git a/REQUIREMENTS b/REQUIREMENTS
new file mode 100644
index 0000000..e69de29

```

We can also see that it set an upstream for our patch branch:
```
$ git branch -vv
* flux-capacitor-power 3e674d1 [rad/patches/a8926643a8f6a65bc386b0131621994000485d4d] Define power requirements
  master               f2de534 [rad/master] Second commit
```

Wait, let's add a README too! Just for fun.

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```
``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch a892664 updated to 8d8aa0887a11f2a37fa8ed0d5723efa96fd727ed
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  flux-capacitor-power -> patches/a8926643a8f6a65bc386b0131621994000485d4d
```

And let's leave a quick comment for our team:

```
$ rad patch comment a8926643a8f6a65bc386b0131621994000485d4d --message 'I cannot wait to get back to the 90s!'
╭───────────────────────────────────────╮
│ z6MknSL…StBU8Vi (you) now b97a27f     │
│ I cannot wait to get back to the 90s! │
╰───────────────────────────────────────╯
$ rad patch comment a8926643a8f6a65bc386b0131621994000485d4d --message 'My favorite decade!' --reply-to b97a27f -q
a3a462bc8ab52e2a6f3568c28a11ba53cf40bbc8
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout a892664
✓ Switched to branch patch/a892664
✓ Branch patch/a892664 setup to track rad/patches/a8926643a8f6a65bc386b0131621994000485d4d
```

We can also add a review verdict as such:

```
$ rad review a8926643a8f6a65bc386b0131621994000485d4d --accept --no-message --no-sync
✓ Patch a892664 accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch show a892664
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     a8926643a8f6a65bc386b0131621994000485d4d                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Branches  flux-capacitor-power, patch/a892664                       │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) now                               │
│ ↑ updated to 8d8aa0887a11f2a37fa8ed0d5723efa96fd727ed (27857ec) now │
│ ✓ accepted by z6MknSL…StBU8Vi (you) now                             │
╰─────────────────────────────────────────────────────────────────────╯
```

If you make a mistake on the patch description, you can always change it!

```
$ rad patch edit a892664 --message "Define power requirements" --message "Add requirements file"
$ rad patch show a892664
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     a8926643a8f6a65bc386b0131621994000485d4d                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Branches  flux-capacitor-power, patch/a892664                       │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add requirements file                                               │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) now                               │
│ ↑ updated to 8d8aa0887a11f2a37fa8ed0d5723efa96fd727ed (27857ec) now │
│ ✓ accepted by z6MknSL…StBU8Vi (you) now                             │
╰─────────────────────────────────────────────────────────────────────╯
```

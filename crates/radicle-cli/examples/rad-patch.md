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

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  70a5938  Define power requirements  alice   (you)  -        602ba44  +0  -0  now             │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show 70a5938a2620ffad0cef086670112c65f69a3d48 -p
╭──────────────────────────────────────────────────────────╮
│ Title     Define power requirements                      │
│ Patch     70a5938a2620ffad0cef086670112c65f69a3d48       │
│ Author    alice (you)                                    │
│ Head      602ba4448210fba26633dc3f9ae3d4d9d20a1e84       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  flux-capacitor-power                           │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ See details.                                             │
├──────────────────────────────────────────────────────────┤
│ 602ba44 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision 70a5938 @ [..   ]..602ba44 by alice (you) now │
╰──────────────────────────────────────────────────────────╯

commit 602ba4448210fba26633dc3f9ae3d4d9d20a1e84
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Define power requirements

diff --git a/REQUIREMENTS b/REQUIREMENTS
new file mode 100644
index 0000000..e69de29

```

We can also list only patches that we've authored.

```
$ rad patch list --authored
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  70a5938  Define power requirements  alice   (you)  -        602ba44  +0  -0  now             │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
```

We can also see that it set an upstream for our patch branch:
```
$ git branch -vv
* flux-capacitor-power 602ba44 [rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48] Define power requirements
  master               4c66f0e [rad/master] Second commit
```

We can also label patches as well as assign DIDs to the patch to help
organise your workflow:

```
$ rad patch label 70a5938 --add fun --no-announce
$ rad patch assign 70a5938 --add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --no-announce
$ rad patch show 70a5938
╭──────────────────────────────────────────────────────────╮
│ Title     Define power requirements                      │
│ Patch     70a5938a2620ffad0cef086670112c65f69a3d48       │
│ Author    alice (you)                                    │
│ Labels    fun                                            │
│ Head      602ba4448210fba26633dc3f9ae3d4d9d20a1e84       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  flux-capacitor-power                           │
│ Commits   ahead 1, behind 0                              │
│ Status    open                                           │
│                                                          │
│ See details.                                             │
├──────────────────────────────────────────────────────────┤
│ 602ba44 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision 70a5938 @ [..   ]..602ba44 by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

Wait, let's add a README too! Just for fun.

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 8083d4c] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```
``` (stderr)
$ git push rad -o patch.message="Add README, just for the fun"
✓ Patch 70a5938 updated to revision f592f45f9d3c326f1632586c82b7845180b6892c
To compare against your previous revision 70a5938, run:

   git range-diff 4c66f0e[..] 602ba44[..] 8083d4c[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   602ba44..8083d4c  flux-capacitor-power -> patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

And let's leave a quick comment for our team:

```
$ rad patch comment 70a5938 --message 'I cannot wait to get back to the 90s!' --no-announce
╭───────────────────────────────────────╮
│ alice (you) now 472d610               │
│ I cannot wait to get back to the 90s! │
╰───────────────────────────────────────╯
$ rad patch comment 70a5938 --message 'My favorite decade!' --reply-to 472d610 -q --no-announce
4c5eebadd04b2725f12e1a907225235389cda33d
```

If we realize we made a mistake in the comment, we can go back and edit it:

```
$ rad patch comment 70a5938 --edit 472d610 --message 'I cannot wait to get back to the 80s!' --no-announce
╭───────────────────────────────────────╮
│ alice (you) now 472d610               │
│ I cannot wait to get back to the 80s! │
╰───────────────────────────────────────╯
```

And if we really made a mistake, then we can redact the comment entirely:

```
$ rad patch comment 70a5938 --redact 472d610 --no-announce
✓ Redacted comment 472d61047d2b56308e32fb5cda17607d6f2d0cd0
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout 70a5938
✓ Switched to branch patch/70a5938 at revision f592f45
✓ Branch patch/70a5938 setup to track rad/patches/70a5938a2620ffad0cef086670112c65f69a3d48
```

We can also add a review verdict as such:

```
$ rad patch review 70a5938 --accept --no-message --no-announce
✓ Patch 70a5938 accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch show 70a5938
╭──────────────────────────────────────────────────────────╮
│ Title     Define power requirements                      │
│ Patch     70a5938a2620ffad0cef086670112c65f69a3d48       │
│ Author    alice (you)                                    │
│ Labels    fun                                            │
│ Head      8083d4cbb2b297ade6a12962f8ddc118c3900dcf       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  flux-capacitor-power, patch/70a5938            │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
│                                                          │
│ See details.                                             │
├──────────────────────────────────────────────────────────┤
│ 8083d4c Add README, just for the fun                     │
│ 602ba44 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision 70a5938 @ [..   ]..602ba44 by alice (you) now │
│ ↑ Revision f592f45 @ [..   ]..8083d4c by alice (you) now │
│   └─ ✓ accepted                       by alice (you) now │
╰──────────────────────────────────────────────────────────╯
$ rad patch list
╭─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated  Labels │
├─────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  70a5938  Define power requirements  alice   (you)  ✓        8083d4c  +0  -0  now      fun    │
╰─────────────────────────────────────────────────────────────────────────────────────────────────╯
```

If you make a mistake on the patch description, you can always change it!

```
$ rad patch edit 70a5938 --message "Define power requirements" --message "Add requirements file" --no-announce
$ rad patch show 70a5938
╭──────────────────────────────────────────────────────────╮
│ Title     Define power requirements                      │
│ Patch     70a5938a2620ffad0cef086670112c65f69a3d48       │
│ Author    alice (you)                                    │
│ Labels    fun                                            │
│ Head      8083d4cbb2b297ade6a12962f8ddc118c3900dcf       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  flux-capacitor-power, patch/70a5938            │
│ Commits   ahead 2, behind 0                              │
│ Status    open                                           │
│                                                          │
│ Add requirements file                                    │
├──────────────────────────────────────────────────────────┤
│ 8083d4c Add README, just for the fun                     │
│ 602ba44 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision 70a5938 @ [..   ]..602ba44 by alice (you) now │
│ ↑ Revision f592f45 @ [..   ]..8083d4c by alice (you) now │
│   └─ ✓ accepted                       by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

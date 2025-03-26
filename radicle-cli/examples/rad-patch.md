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
✓ Patch aa45913e757cacd46972733bddee5472c78fa32a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  aa45913  Define power requirements  alice   (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show aa45913e757cacd46972733bddee5472c78fa32a -p
╭────────────────────────────────────────────────────╮
│ Title     Define power requirements                │
│ Patch     aa45913e757cacd46972733bddee5472c78fa32a │
│ Author    alice (you)                              │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33 │
│ Branches  flux-capacitor-power                     │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
│                                                    │
│ See details.                                       │
├────────────────────────────────────────────────────┤
│ 3e674d1 Define power requirements                  │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now              │
╰────────────────────────────────────────────────────╯

commit 3e674d1a1df90807e934f9ae5da2591dd6848a33
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
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  aa45913  Define power requirements  alice   (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

We can also see that it set an upstream for our patch branch:
```
$ git branch -vv
* flux-capacitor-power 3e674d1 [rad/patches/aa45913e757cacd46972733bddee5472c78fa32a] Define power requirements
  master               f2de534 [rad/master] Second commit
```

We can also label patches as well as assign DIDs to the patch to help
organise your workflow:

```
$ rad patch label aa45913 --add fun --no-announce
$ rad patch assign aa45913 --add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --no-announce
$ rad patch show aa45913
╭────────────────────────────────────────────────────╮
│ Title     Define power requirements                │
│ Patch     aa45913e757cacd46972733bddee5472c78fa32a │
│ Author    alice (you)                              │
│ Labels    fun                                      │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33 │
│ Branches  flux-capacitor-power                     │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
│                                                    │
│ See details.                                       │
├────────────────────────────────────────────────────┤
│ 3e674d1 Define power requirements                  │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now              │
╰────────────────────────────────────────────────────╯
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
✓ Patch aa45913 updated to revision 6e5a3b7b2ce27b32e7ccc2f0b3f4594897dde638
To compare against your previous revision aa45913, run:

   git range-diff f2de534[..] 3e674d1[..] 27857ec[..]

To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  flux-capacitor-power -> patches/aa45913e757cacd46972733bddee5472c78fa32a
```

And let's leave a quick comment for our team:

```
$ rad patch comment aa45913 --message 'I cannot wait to get back to the 90s!' --no-announce
╭───────────────────────────────────────╮
│ alice (you) now a6202ea               │
│ I cannot wait to get back to the 90s! │
╰───────────────────────────────────────╯
$ rad patch comment aa45913 --message 'My favorite decade!' --reply-to a6202ea -q --no-announce
5880c53b2b34d4eb9f9bee0f44cb23fbaa9c021e
```

If we realize we made a mistake in the comment, we can go back and edit it:

```
$ rad patch comment aa45913 --edit a6202ea --message 'I cannot wait to get back to the 80s!' --no-announce
╭───────────────────────────────────────╮
│ alice (you) now a6202ea               │
│ I cannot wait to get back to the 80s! │
╰───────────────────────────────────────╯
```

And if we really made a mistake, then we can redact the comment entirely:

```
$ rad patch comment aa45913 --redact 5880c53b2b34d4eb9f9bee0f44cb23fbaa9c021e --no-announce
✓ Redacted comment 5880c53b2b34d4eb9f9bee0f44cb23fbaa9c021e
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout aa45913
✓ Switched to branch patch/aa45913 at revision 6e5a3b7
✓ Branch patch/aa45913 setup to track rad/patches/aa45913e757cacd46972733bddee5472c78fa32a
```

We can also add a review verdict as such:

```
$ rad patch review aa45913 --accept --no-message --no-announce
✓ Patch aa45913 accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch show aa45913
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     aa45913e757cacd46972733bddee5472c78fa32a                  │
│ Author    alice (you)                                               │
│ Labels    fun                                                       │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Branches  flux-capacitor-power, patch/aa45913                       │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now                               │
│ ↑ updated to 6e5a3b7b2ce27b32e7ccc2f0b3f4594897dde638 (27857ec) now │
│   └─ ✓ accepted by alice (you) now                                  │
╰─────────────────────────────────────────────────────────────────────╯
$ rad patch list
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  aa45913  Define power requirements  alice   (you)  ✔        27857ec  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

If you make a mistake on the patch description, you can always change it!

```
$ rad patch edit aa45913 --message "Define power requirements" --message "Add requirements file" --no-announce
$ rad patch show aa45913
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     aa45913e757cacd46972733bddee5472c78fa32a                  │
│ Author    alice (you)                                               │
│ Labels    fun                                                       │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Branches  flux-capacitor-power, patch/aa45913                       │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add requirements file                                               │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now                               │
│ ↑ updated to 6e5a3b7b2ce27b32e7ccc2f0b3f4594897dde638 (27857ec) now │
│   └─ ✓ accepted by alice (you) now                                  │
╰─────────────────────────────────────────────────────────────────────╯
```

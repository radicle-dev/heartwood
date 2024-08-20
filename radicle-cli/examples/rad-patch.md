When contributing to another's project, it is common for the contribution to be
of many commits and involve a discussion with the project's maintainer.  This is supported
via Radicle's Patches.

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
✓ Patch c90967c43719b916e0b5a8b5dafe353608f8a08a opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  c90967c  Define power requirements  alice   (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show c90967c43719b916e0b5a8b5dafe353608f8a08a -p
╭────────────────────────────────────────────────────╮
│ Title     Define power requirements                │
│ Patch     c90967c43719b916e0b5a8b5dafe353608f8a08a │
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
│ ●  c90967c  Define power requirements  alice   (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

We can also see that it set an upstream for our patch branch:
```
$ git branch -vv
* flux-capacitor-power 3e674d1 [rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a] Define power requirements
  master               f2de534 [rad/master] Second commit
```

We can also label patches as well as assign DIDs to the patch to help
organise your workflow:

```
$ rad patch label c90967c --add fun --no-announce
$ rad patch assign c90967c --add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --no-announce
$ rad patch show c90967c
╭────────────────────────────────────────────────────╮
│ Title     Define power requirements                │
│ Patch     c90967c43719b916e0b5a8b5dafe353608f8a08a │
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
✓ Patch c90967c updated to revision a7fe44ba5d9c2339b0e9731874791db375aeebbe
To compare against your previous revision c90967c, run:

   git range-diff f2de534[..] 3e674d1[..] 27857ec[..]

To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
   3e674d1..27857ec  flux-capacitor-power -> patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

And let's leave a quick comment for our team:

```
$ rad patch comment c90967c --message 'I cannot wait to get back to the 90s!' --no-announce
╭───────────────────────────────────────╮
│ alice (you) now 055f0d2               │
│ I cannot wait to get back to the 90s! │
╰───────────────────────────────────────╯
$ rad patch comment c90967c --message 'My favorite decade!' --reply-to 055f0d2 -q --no-announce
84336c0ffd31e607839d2f4dd3389556dd766124
```

If we realize we made a mistake in the comment, we can go back and edit it:

```
$ rad patch comment c90967c --edit 055f0d2 --message 'I cannot wait to get back to the 80s!' --no-announce
╭───────────────────────────────────────╮
│ alice (you) now 055f0d2               │
│ I cannot wait to get back to the 80s! │
╰───────────────────────────────────────╯
```

And if we really made a mistake, then we can redact the comment entirely:

```
$ rad patch comment c90967c --redact 84336c0 --no-announce
✓ Redacted comment 84336c0ffd31e607839d2f4dd3389556dd766124
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout c90967c
✓ Switched to branch patch/c90967c at revision a7fe44b
✓ Branch patch/c90967c setup to track rad/patches/c90967c43719b916e0b5a8b5dafe353608f8a08a
```

We can also add a review verdict as such:

```
$ rad patch review c90967c --accept --no-message --no-announce
✓ Patch c90967c accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch show c90967c
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     c90967c43719b916e0b5a8b5dafe353608f8a08a                  │
│ Author    alice (you)                                               │
│ Labels    fun                                                       │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Branches  flux-capacitor-power, patch/c90967c                       │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now                               │
│ ↑ updated to a7fe44ba5d9c2339b0e9731874791db375aeebbe (27857ec) now │
│   └─ ✓ accepted by alice (you) now                                  │
╰─────────────────────────────────────────────────────────────────────╯
$ rad patch list
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  c90967c  Define power requirements  alice   (you)  ✔        27857ec  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

If you make a mistake on the patch description, you can always change it!

```
$ rad patch edit c90967c --message "Define power requirements" --message "Add requirements file" --no-announce
$ rad patch show c90967c
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     c90967c43719b916e0b5a8b5dafe353608f8a08a                  │
│ Author    alice (you)                                               │
│ Labels    fun                                                       │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                  │
│ Branches  flux-capacitor-power, patch/c90967c                       │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ Add requirements file                                               │
├─────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now                               │
│ ↑ updated to a7fe44ba5d9c2339b0e9731874791db375aeebbe (27857ec) now │
│   └─ ✓ accepted by alice (you) now                                  │
╰─────────────────────────────────────────────────────────────────────╯
```

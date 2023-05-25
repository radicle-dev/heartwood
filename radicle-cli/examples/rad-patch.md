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

```
$ rad patch open --message "Define power requirements" --message "See details."
master <- z6MknSL…StBU8Vi/flux-capacitor-power (3e674d1)
1 commit(s) ahead, 0 commit(s) behind

3e674d1 Define power requirements

✓ Patch 077e4bbe9a6e5546f400ef5951768c37a76f13a4 created

To publish your patch to the network, run:
    git push rad
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author                  Head     +   -   Updated      │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  077e4bb  Define power requirements  z6MknSL…StBU8Vi  (you)  3e674d1  +0  -0  [   ...    ] │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show 077e4bbe9a6e5546f400ef5951768c37a76f13a4 -p
╭────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                │
│ Patch     077e4bbe9a6e5546f400ef5951768c37a76f13a4                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33                 │
│ Branches  flux-capacitor-power                                     │
│ Commits   ahead 1, behind 0                                        │
│ Status    open                                                     │
│                                                                    │
│ See details.                                                       │
├────────────────────────────────────────────────────────────────────┤
│ 3e674d1 Define power requirements                                  │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) (z6MknSL…StBU8Vi) [    ...    ]                  │
╰────────────────────────────────────────────────────────────────────╯

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
* flux-capacitor-power 3e674d1 [rad/flux-capacitor-power] Define power requirements
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
$ rad patch update --message "Add README, just for the fun" 077e4bbe9a6e5546f400ef5951768c37a76f13a4
Updating 3e674d1 -> 27857ec
1 commit(s) ahead, 0 commit(s) behind
✓ Patch updated to revision 5cdcd2e14411e2bfec7b11bcf4667e2e0fc4d417
```

And let's leave a quick comment for our team:

```
$ rad comment 077e4bbe9a6e5546f400ef5951768c37a76f13a4 --message 'I cannot wait to get back to the 90s!'
31a07b8e7758af2027e74e521a74bea4574280e7
$ rad comment 077e4bbe9a6e5546f400ef5951768c37a76f13a4 --message 'I cannot wait to get back to the 90s!' --reply-to 31a07b8e7758af2027e74e521a74bea4574280e7
d66bcb6bfe2e06e57636e8b1ba3ef8098a8bb250
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout 077e4bb
✓ Switched to branch patch/077e4bb
```

We can also add a review verdict as such:

```
$ rad review 077e4bbe9a6e5546f400ef5951768c37a76f13a4 --accept --no-message --no-sync
✓ Patch 077e4bb accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch show 077e4bb
╭──────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                          │
│ Patch     077e4bbe9a6e5546f400ef5951768c37a76f13a4                           │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi           │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                           │
│ Branches  flux-capacitor-power, patch/077e4bb                                │
│ Commits   ahead 2, behind 0                                                  │
│ Status    open                                                               │
│                                                                              │
│ See details.                                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                         │
│ 3e674d1 Define power requirements                                            │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) (z6MknSL…StBU8Vi) [   ...    ]                             │
│ ↑ updated to 5cdcd2e14411e2bfec7b11bcf4667e2e0fc4d417 (27857ec) [   ...    ] │
│ ✓ accepted by (you) (z6MknSL…StBU8Vi) [   ...    ]                           │
╰──────────────────────────────────────────────────────────────────────────────╯
```

If you make a mistake on the patch description, you can always change it!

```
$ rad patch edit 077e4bb --message "Define power requirements" --message "Add requirements file"
$ rad patch show 077e4bb
╭──────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                          │
│ Patch     077e4bbe9a6e5546f400ef5951768c37a76f13a4                           │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi           │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                           │
│ Branches  flux-capacitor-power, patch/077e4bb                                │
│ Commits   ahead 2, behind 0                                                  │
│ Status    open                                                               │
│                                                                              │
│ Add requirements file                                                        │
├──────────────────────────────────────────────────────────────────────────────┤
│ 27857ec Add README, just for the fun                                         │
│ 3e674d1 Define power requirements                                            │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) (z6MknSL…StBU8Vi) [   ...    ]                             │
│ ↑ updated to 5cdcd2e14411e2bfec7b11bcf4667e2e0fc4d417 (27857ec) [   ...    ] │
│ ✓ accepted by (you) (z6MknSL…StBU8Vi) [   ...    ]                           │
╰──────────────────────────────────────────────────────────────────────────────╯
```

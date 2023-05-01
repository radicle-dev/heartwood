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

✓ Patch de3096d5cc422136016ac210b870bfa9d0f11481 created

To publish your patch to the network, run:
    git push rad
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author                  Head     +   -   Opened       │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  de3096d  Define power requirements  z6MknSL…StBU8Vi  (you)  3e674d1  +0  -0  4 months ago │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show de3096d5cc422136016ac210b870bfa9d0f11481 -p
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                                     │
│ Patch     de3096d5cc422136016ac210b870bfa9d0f11481                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33                                      │
│ Branches  flux-capacitor-power                                                          │
│ Commits   ahead 1, behind 0                                                             │
│ Status    open                                                                          │
│                                                                                         │
│ See details.                                                                            │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [    ...    ]│
╰─────────────────────────────────────────────────────────────────────────────────────────╯

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
$ rad patch update --message "Add README, just for the fun" de3096d5cc422136016ac210b870bfa9d0f11481
Updating 3e674d1 -> 27857ec
1 commit(s) ahead, 0 commit(s) behind
✓ Patch updated to revision d00f978a43a255c7f2f9f23d39b555d103900c6d
```

And let's leave a quick comment for our team:

```
$ rad comment de3096d5cc422136016ac210b870bfa9d0f11481 --message 'I cannot wait to get back to the 90s!'
225353d1b9195f6cf4cfe098ce7935d4c933c36e
$ rad comment de3096d5cc422136016ac210b870bfa9d0f11481 --message 'I cannot wait to get back to the 90s!' --reply-to 225353d1b9195f6cf4cfe098ce7935d4c933c36e
089c2d74d18036c75b1b4d3a32770c720a6967e2
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout de3096d
✓ Switched to branch patch/de3096d
```

We can also add a review verdict as such:

```
$ rad review de3096d5cc422136016ac210b870bfa9d0f11481 --accept --no-message --no-sync
✓ Patch de3096d accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch show de3096d
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                                     │
│ Patch     de3096d5cc422136016ac210b870bfa9d0f11481                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                                      │
│ Branches  flux-capacitor-power, patch/de3096d                                           │
│ Commits   ahead 2, behind 0                                                             │
│ Status    open                                                                          │
│                                                                                         │
│ See details.                                                                            │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [    ...    ]│
│ ↑ updated to d00f978a43a255c7f2f9f23d39b555d103900c6d (27857ec) [               ...    ]│
│ ✓ accepted by z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [          ...    ]│
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

If you make a mistake on the patch description, you can always change it!

```
$ rad patch edit de3096d --message "Define power requirements" --message "Add requirements file"
$ rad patch show de3096d
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                                     │
│ Patch     de3096d5cc422136016ac210b870bfa9d0f11481                                      │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                      │
│ Head      27857ec9eb04c69cacab516e8bf4b5fd36090f66                                      │
│ Branches  flux-capacitor-power, patch/de3096d                                           │
│ Commits   ahead 2, behind 0                                                             │
│ Status    open                                                                          │
│                                                                                         │
│ Add requirements file                                                                   │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [    ...    ]│
│ ↑ updated to d00f978a43a255c7f2f9f23d39b555d103900c6d (27857ec) [               ...    ]│
│ ✓ accepted by z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [          ...    ]│
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

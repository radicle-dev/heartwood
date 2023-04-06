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

✓ Patch 191a14e520f2eeff7c0e3ee0a5523c5217eecb89 created 🌱

To publish your patch to the network, run:
    rad push

```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Define power requirements 191a14e R0 3e674d1 (flux-capacitor-power) ahead 1, behind 0   │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) 3 months ago │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```
```
$ rad patch show 191a14e520f2eeff7c0e3ee0a5523c5217eecb89
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title   Define power requirements                                                       │
│ Patch   191a14e520f2eeff7c0e3ee0a5523c5217eecb89                                        │
│ Author  did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi                        │
│ Status  open                                                                            │
│                                                                                         │
│ See details.                                                                            │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) 3 months ago │
╰─────────────────────────────────────────────────────────────────────────────────────────╯

commit 3e674d1a1df90807e934f9ae5da2591dd6848a33
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Define power requirements

diff --git a/REQUIREMENTS b/REQUIREMENTS
new file mode 100644
index 0000000..e69de29

```

Wait, let's add a README too! Just for fun.

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
$ rad patch update --message "Add README, just for the fun" 191a14e520f2eeff7c0e3ee0a5523c5217eecb89
Updating 3e674d1 -> 27857ec
1 commit(s) ahead, 0 commit(s) behind
✓ Patch updated to revision 56a5efc062b34a97a162a3a4d4468ce3e7ba1c84
```

And let's leave a quick comment for our team:

```
$ rad comment 191a14e520f2eeff7c0e3ee0a5523c5217eecb89 --message 'I cannot wait to get back to the 90s!'
70fc8b18300096f6f0f919797457244e6e4b2cea
$ rad comment 191a14e520f2eeff7c0e3ee0a5523c5217eecb89 --message 'I cannot wait to get back to the 90s!' --reply-to 70fc8b18300096f6f0f919797457244e6e4b2cea
7a9f7a6358238f4ff115d2b2a5e522ab93867d38
```

Now, let's checkout the patch that we just created:

```
$ rad patch checkout 191a14e52
✓ Switched to branch patch/191a14e
```

We can also add a review verdict as such:

```
$ rad review 191a14e520f2eeff7c0e3ee0a5523c5217eecb89 --accept --no-message --no-sync
✓ Patch 191a14e accepted
```

Showing the patch list now will reveal the favorable verdict:

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Define power requirements 191a14e R1 27857ec (flux-capacitor-power, patch/191a14e) ahead 2, behind 0 │
├──────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) 3 months ago              │
│ ↑ updated to 56a5efc062b34a97a162a3a4d4468ce3e7ba1c84 (27857ec) 3 months ago                         │
│ ✓ accepted by z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) 3 months ago                    │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

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
$ rad patch open --message "define power requirements" --no-confirm

🌱 Creating patch for heartwood

ok Pushing HEAD to storage...
ok Analyzing remotes...

z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master (f2de534) <- z6MknSL…StBU8Vi/flux-capacitor-power (3e674d1)
1 commit(s) ahead, 0 commit(s) behind

3e674d1 Define power requirements


╭─ define power requirements ───────

No description provided.

╰───────────────────────────────────


ok Patch d4ef85f57a849bd845915d7a66a2192cd23811f6 created 🌱
```

It will now be listed as one of the project's open patches.

```
$ rad patch

- YOU PROPOSED -

define power requirements d4ef85f57a8 R0 3e674d1 (flux-capacitor-power) ahead 1, behind 0
└─ * opened by z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [..]
└─ * patch id d4ef85f57a849bd845915d7a66a2192cd23811f6

- OTHERS PROPOSED -

Nothing to show.

$ rad patch show d4ef85f57a849bd845915d7a66a2192cd23811f6

patch d4ef85f57a849bd845915d7a66a2192cd23811f6

╭─ define power requirements ───────

No description provided.

╰───────────────────────────────────

commit 3e674d1a1df90807e934f9ae5da2591dd6848a33
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Define power requirements

diff --git a/REQUIREMENTS b/REQUIREMENTS
new file mode 100644
index 0000000..e69de29

```

Wait, lets add a README too! Just for fun.

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
$ rad patch update --message "Add README, just for the fun" --no-confirm d4ef85f57a849bd845915d7a66a2192cd23811f6

🌱 Updating patch for heartwood

ok Pushing HEAD to storage...
ok Analyzing remotes...

d4ef85f57a8 R0 (3e674d1) -> R1 (27857ec)
1 commit(s) ahead, 0 commit(s) behind


ok Patch d4ef85f57a849bd845915d7a66a2192cd23811f6 updated 🌱

```

And lets leave a quick comment for our team:

```
$ rad comment d4ef85f57a849bd845915d7a66a2192cd23811f6 --message 'I cannot wait to get back to the 90s!'
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/5
$ rad comment d4ef85f57a849bd845915d7a66a2192cd23811f6 --message 'I cannot wait to get back to the 90s!' --reply-to z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/5
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/6
```

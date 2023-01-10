When contributing to another's project, it is common for the contribution to be
of many commits and involve a discussion with the project's maintainer.  This is supported
via Radicle's patches.

Here we give a brief overview for using patches in our hypothetical car
scenario.  It turns out instructions containing the power requirements were
missing from the project.

```
$ git checkout -b flux-capacitor-power
$ touch README.md
```

Here the instructions are added to the project's README for 1.21 gigawatts and
commit the changes to git.

```
$ git add README.md
$ git commit -m "Define power requirements"
[flux-capacitor-power d0f7633] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
```

Once the code is ready, we open (or create) a patch with our changes for the project.

```
$ rad patch open --message "define power requirements" --no-confirm

🌱 Creating patch for heartwood

ok Pushing HEAD to storage...
ok Analyzing remotes...

z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/master (cdf76ce) <- z6MknSL…StBU8Vi/flux-capacitor-power (d0f7633)
1 commit(s) ahead, 0 commit(s) behind

d0f7633 Define power requirements


╭─ define power requirements ───────

No description provided.

╰───────────────────────────────────


ok Patch 93a11d4883c8e3e6d37b401b8e6699a397df464c created 🌱
```

It will now be listed as one of the project's open patches.

```
$ rad patch

- YOU PROPOSED -

define power requirements 93a11d4883c R0 d0f7633 (flux-capacitor-power) ahead 1, behind 0
└─ * opened by z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi (you) [..]

- OTHERS PROPOSED -

Nothing to show.

```

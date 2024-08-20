When contributing to another's project, it is common for the contribution to be
of many commits and involve a discussion with the project's maintainer.  This is supported
via Radicle *patches*.

Here we give a brief overview for using patches in our hypothetical car
scenario.  It turns out instructions containing the power requirements were
missing from the project.

```
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
```

Here the instructions are added to the project's `REQUIREMENTS` for 1.21
gigawatts and committed with git.

```
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
```

Once the code is ready, we open a patch with our changes.

``` (stderr)
$ git push rad -o no-sync -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
✓ Patch 3aa3bbfbc4162e34ab6787b3508e7ec84166d182 opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  3aa3bbf  Define power requirements  bob     (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
$ rad patch show 3aa3bbfbc4162e34ab6787b3508e7ec84166d182
╭────────────────────────────────────────────────────╮
│ Title     Define power requirements                │
│ Patch     3aa3bbfbc4162e34ab6787b3508e7ec84166d182 │
│ Author    bob (you)                                │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33 │
│ Branches  flux-capacitor-power                     │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
│                                                    │
│ See details.                                       │
├────────────────────────────────────────────────────┤
│ 3e674d1 Define power requirements                  │
├────────────────────────────────────────────────────┤
│ ● opened by bob (you) (3e674d1) now                │
╰────────────────────────────────────────────────────╯
```

We can also confirm that the patch branch is in storage:

```
$ git ls-remote rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk refs/heads/patches/*
3e674d1a1df90807e934f9ae5da2591dd6848a33	refs/heads/patches/3aa3bbfbc4162e34ab6787b3508e7ec84166d182
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
``` (stderr) RAD_SOCKET=/dev/null
$ git push -o patch.message="Add README, just for the fun"
✓ Patch 3aa3bbf updated to revision 8ea87be8cb7d590f381338348532200b230368af
To compare against your previous revision 3aa3bbf, run:

   git range-diff f2de534[..] 3e674d1[..] 27857ec[..]

To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   3e674d1..27857ec  flux-capacitor-power -> patches/3aa3bbfbc4162e34ab6787b3508e7ec84166d182
```

And let's leave a quick comment for our team:

```
$ rad patch comment 3aa3bbfbc4162e34ab6787b3508e7ec84166d182 --message 'I cannot wait to get back to the 90s!' -q
528abde17e16bef2aa12157c745a9a74e4005051
✓ Synced with 1 node(s)
```

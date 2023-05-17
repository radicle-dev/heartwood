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

```
$ rad patch open --message "Define power requirements" --message "See details."
master <- z6Mkt67…v4N1tRk/flux-capacitor-power (3e674d1)
1 commit(s) ahead, 0 commit(s) behind

3e674d1 Define power requirements

✓ Patch 5f0a547f7a91bf002bb0542035a647fd5af134a5 created

To publish your patch to the network, run:
    git push rad
```

It will now be listed as one of the project's open patches.

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author                  Head     +   -   Updated      │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  5f0a547  Define power requirements  z6Mkt67…v4N1tRk  (you)  3e674d1  +0  -0  [    ...   ] │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
$ rad patch show 5f0a547f7a91bf002bb0542035a647fd5af134a5
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                                     │
│ Patch     5f0a547f7a91bf002bb0542035a647fd5af134a5                                      │
│ Author    did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk                      │
│ Head      3e674d1a1df90807e934f9ae5da2591dd6848a33                                      │
│ Branches  flux-capacitor-power                                                          │
│ Commits   ahead 1, behind 0                                                             │
│ Status    open                                                                          │
│                                                                                         │
│ See details.                                                                            │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ 3e674d1 Define power requirements                                                       │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ● opened by did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk (you) [    ...    ]│
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

We can also confirm that the patch branch is in storage:

```
$ git ls-remote rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk refs/heads/flux-capacitor-power
3e674d1a1df90807e934f9ae5da2591dd6848a33	refs/heads/flux-capacitor-power
```

Wait, let's add a README too! Just for fun.

```
$ touch README.md
$ git add README.md
$ git commit --message "Add README, just for the fun"
[flux-capacitor-power 27857ec] Add README, just for the fun
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 README.md
$ rad patch update --message "Add README, just for the fun" 5f0a547f7a91bf002bb0542035a647fd5af134a5
Updating 3e674d1 -> 27857ec
1 commit(s) ahead, 0 commit(s) behind
✓ Patch updated to revision b7e2356fb7e3981980b42603eea969851d17a40d
```

And let's leave a quick comment for our team:

```
$ rad comment 5f0a547f7a91bf002bb0542035a647fd5af134a5 --message 'I cannot wait to get back to the 90s!'
a15e976e4273971d6695eff2e07a57a82133567f
```

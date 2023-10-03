Handle arbitrary COBs.

First create an issue.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

The issue is now listed under our project.

```
$ rad issue list
╭─────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author                    Labels   Assignees   Opened       │
├─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   9bf82c1   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)                        [    ..    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Let's create a patch, too.

```
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
```

Patch can be listed.

```
$ rad patch
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author                  Head     +   -   Updated      │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  a892664  Define power requirements  z6MknSL…StBU8Vi  (you)  3e674d1  +0  -0  [   ...    ] │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue
9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch
a8926643a8f6a65bc386b0131621994000485d4d
```

We can look at the issue COB.

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d
commit 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d
parent 2317f74de0494c489a233ca6f29f2b8bff6d4f15
author z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date   Thu, 15 Dec 2022 17:28:04 +0000

    {
      "body": "Flux capacitor power requirements exceed current supply",
      "type": "comment"
    }

    {
      "assignees": [],
      "type": "assign"
    }

    {
      "title": "flux capacitor underpowered",
      "type": "edit"
    }

    {
      "labels": [],
      "type": "label"
    }

```

We can look at the patch COB too.

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object a8926643a8f6a65bc386b0131621994000485d4d
commit a8926643a8f6a65bc386b0131621994000485d4d
parent 2317f74de0494c489a233ca6f29f2b8bff6d4f15
parent 3e674d1a1df90807e934f9ae5da2591dd6848a33
parent f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
author z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date   Thu, 15 Dec 2022 17:28:04 +0000

    {
      "base": "f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354",
      "description": "See details.",
      "oid": "3e674d1a1df90807e934f9ae5da2591dd6848a33",
      "type": "revision"
    }

    {
      "target": "delegates",
      "title": "Define power requirements",
      "type": "edit"
    }

    {
      "labels": [],
      "type": "label"
    }

```

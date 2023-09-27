Handle arbitrary COBs.

First create an issue.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   42028af21fabc09bfac2f25490f119f7c7e11542        │
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
│ ●   42028af   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)                        [    ..    ] │
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
│ ●  73b73f3  Define power requirements  z6MknSL…StBU8Vi  (you)  3e674d1  +0  -0  [   ...    ] │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue
42028af21fabc09bfac2f25490f119f7c7e11542
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch
73b73f376e93e09e0419664766ac9e433bf7d389
```

We can look at the issue COB.

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object 42028af21fabc09bfac2f25490f119f7c7e11542
commit 42028af21fabc09bfac2f25490f119f7c7e11542
parent 175267b8910895ba87760313af254c2900743912
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
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 73b73f376e93e09e0419664766ac9e433bf7d389
commit 73b73f376e93e09e0419664766ac9e433bf7d389
parent 175267b8910895ba87760313af254c2900743912
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

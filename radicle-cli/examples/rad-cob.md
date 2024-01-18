Handle arbitrary COBs.

First create an issue.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

The issue is now listed under our project.

```
$ rad issue list
╭───────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author                    Labels   Assignees   Opened │
├───────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d185ee1   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)                        now    │
╰───────────────────────────────────────────────────────────────────────────────────────────────────╯
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
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author                  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  0f3cd0b  Define power requirements  z6MknSL…StBU8Vi  (you)  3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue
d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch
0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
```

We can look at the issue COB.

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
commit   d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

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
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object 0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
commit   0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f
resource 0656c217f917c3e06234771e9ecae53aba5e173e
rel      3e674d1a1df90807e934f9ae5da2591dd6848a33
rel      f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

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

Finally let's updated the issue and see the `parent` header:

```
$ rad issue label d185ee1 --add bug --no-announce
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
commit   4dd9a3dfb60665e427a43dbf289e50b7fb90a655
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "labels": [
        "bug"
      ],
      "type": "label"
    }

commit   d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

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

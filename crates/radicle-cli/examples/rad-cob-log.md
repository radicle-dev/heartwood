Handle arbitrary COBs.

First create an issue.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe        │
│ Author  alice (you)                                     │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

The issue is now listed under our project.

```
$ rad issue list
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels   Assignees   Opened │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d87dcfe   flux capacitor underpowered   alice    (you)                        now    │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
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
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  aa45913  Define power requirements  alice   (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue
d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
$ rad cob list --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch
aa45913e757cacd46972733bddee5472c78fa32a
```

We can look at the issue COB.

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
commit   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "body": "Flux capacitor power requirements exceed current supply",
      "type": "comment"
    }

    {
      "title": "flux capacitor underpowered",
      "type": "edit"
    }

```

We can look at the patch COB too.

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.patch --object aa45913e757cacd46972733bddee5472c78fa32a
commit   aa45913e757cacd46972733bddee5472c78fa32a
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

```

Finally let's updated the issue and see the `parent` header:

```
$ rad issue label d87dcfe --add bug --no-announce
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
commit   abec0a9f3c945594c4e78d24d8ec679e56b22b79
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "labels": [
        "bug"
      ],
      "type": "label"
    }

commit   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "body": "Flux capacitor power requirements exceed current supply",
      "type": "comment"
    }

    {
      "title": "flux capacitor underpowered",
      "type": "edit"
    }

```

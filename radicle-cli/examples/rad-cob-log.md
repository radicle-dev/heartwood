Handle arbitrary COBs.

First create an issue.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   0d18c610be2fbb4f47d45434c581f3bf0b0ff071        │
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
│ ●   0d18c61   flux capacitor underpowered   alice    (you)                        now    │
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
│ ●  c90967c  Define power requirements  alice   (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

Both issue and patch COBs can be listed.

```
$ rad cob list --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.issue
0d18c610be2fbb4f47d45434c581f3bf0b0ff071
$ rad cob list --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch
c90967c43719b916e0b5a8b5dafe353608f8a08a
```

We can look at the issue COB.

```
$ rad cob log --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.issue --object 0d18c610be2fbb4f47d45434c581f3bf0b0ff071
commit   0d18c610be2fbb4f47d45434c581f3bf0b0ff071
resource eeb8b44890570ccf85db7f3cb2a475100a27408a
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
$ rad cob log --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.patch --object c90967c43719b916e0b5a8b5dafe353608f8a08a
commit   c90967c43719b916e0b5a8b5dafe353608f8a08a
resource eeb8b44890570ccf85db7f3cb2a475100a27408a
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
$ rad issue label 0d18c61 --add bug --no-announce
$ rad cob log --repo rad:z3W5xAVWJ9Gc4LbN16mE3tjWX92t2 --type xyz.radicle.issue --object 0d18c610be2fbb4f47d45434c581f3bf0b0ff071
commit   c1cde09b836c4b1dc25acbcf73105b3794df84d8
resource eeb8b44890570ccf85db7f3cb2a475100a27408a
parent   0d18c610be2fbb4f47d45434c581f3bf0b0ff071
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "labels": [
        "bug"
      ],
      "type": "label"
    }

commit   0d18c610be2fbb4f47d45434c581f3bf0b0ff071
resource eeb8b44890570ccf85db7f3cb2a475100a27408a
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

The `rad cob` command provides a subcommand, `log`, for inspecting the
operations of a COB.

To demonstrate, we will first create an issue and interact with it:

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
$ rad issue react d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --to d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --emoji ✨ --no-announce
$ rad issue comment d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --message "Max power!" --no-announce
╭─────────────────────────╮
│ alice (you) now 3c849c9 │
│ Max power!              │
╰─────────────────────────╯
$ rad issue assign d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --add did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk --no-announce
```

Now, let's see the list of operations using `rad cob log`:

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
commit   376ba71113603004eae3c1b125c58cdc41d36b73
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "assign",
      "assignees": [
        "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ]
    }

commit   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   256908937f3cda8df522d5a3ba442eb935c3f11b
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }

commit   256908937f3cda8df522d5a3ba442eb935c3f11b
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }

commit   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Flux capacitor power requirements exceed current supply"
    }

    {
      "type": "edit",
      "title": "flux capacitor underpowered"
    }

```

We can also limit the range of operations, using the `--from` and `--until`
options. We will need some commit revisions to use for those options, so let's
look at what those revision are by using `rad cob log`:

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
commit   376ba71113603004eae3c1b125c58cdc41d36b73
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "assign",
      "assignees": [
        "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ]
    }

commit   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   256908937f3cda8df522d5a3ba442eb935c3f11b
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }

commit   256908937f3cda8df522d5a3ba442eb935c3f11b
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }

commit   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Flux capacitor power requirements exceed current supply"
    }

    {
      "type": "edit",
      "title": "flux capacitor underpowered"
    }

```

If we provide only the `--from` option, the operations we get back start from that
revision and go until the end:

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --from 3c849c9b555b18be9a1f6c71fb254ba000de8cfe
commit   376ba71113603004eae3c1b125c58cdc41d36b73
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "assign",
      "assignees": [
        "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ]
    }

commit   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   256908937f3cda8df522d5a3ba442eb935c3f11b
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }

```

Conversely, if we provide only the `--until` option, the operations we get back
start from the beginning and stop at that revision:

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --until 256908937f3cda8df522d5a3ba442eb935c3f11b
commit   256908937f3cda8df522d5a3ba442eb935c3f11b
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }

commit   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Flux capacitor power requirements exceed current supply"
    }

    {
      "type": "edit",
      "title": "flux capacitor underpowered"
    }

```

Finally, if we provide both, we get back that exact range:

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --from 256908937f3cda8df522d5a3ba442eb935c3f11b --until 3c849c9b555b18be9a1f6c71fb254ba000de8cfe
commit   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   256908937f3cda8df522d5a3ba442eb935c3f11b
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }

commit   256908937f3cda8df522d5a3ba442eb935c3f11b
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }

```

The `rad cob` command provides a subcommand, `actions`, for inspecting the
actions of a COB.

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

Now, let's see the list of actions using `rad cob actions`:

```
$ rad cob actions --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
{
  "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
  "actions": [
    {
      "type": "comment",
      "body": "Flux capacitor power requirements exceed current supply"
    },
    {
      "type": "edit",
      "title": "flux capacitor underpowered"
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
{
  "id": "256908937f3cda8df522d5a3ba442eb935c3f11b",
  "actions": [
    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
{
  "id": "3c849c9b555b18be9a1f6c71fb254ba000de8cfe",
  "actions": [
    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "256908937f3cda8df522d5a3ba442eb935c3f11b"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
{
  "id": "376ba71113603004eae3c1b125c58cdc41d36b73",
  "actions": [
    {
      "type": "assign",
      "assignees": [
        "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ]
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "3c849c9b555b18be9a1f6c71fb254ba000de8cfe"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
```

We can also limit the range of actions, using the `--from` and `--until`
options. We will need some commit revisions to use for those options, so let's
look at what they are using `rad cob log`:

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
commit   376ba71113603004eae3c1b125c58cdc41d36b73
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "assignees": [
        "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ],
      "type": "assign"
    }

commit   3c849c9b555b18be9a1f6c71fb254ba000de8cfe
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   256908937f3cda8df522d5a3ba442eb935c3f11b
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "type": "comment"
    }

commit   256908937f3cda8df522d5a3ba442eb935c3f11b
resource 0656c217f917c3e06234771e9ecae53aba5e173e
parent   d87dcfe8c2b3200e78b128d9b959cfdf7063fefe
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "active": true,
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "type": "comment.react"
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

If we provide only the `--from` option, the actions we get back start from that
revision and go until the end:

```
$ rad cob actions --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --from 3c849c9b555b18be9a1f6c71fb254ba000de8cfe
{
  "id": "3c849c9b555b18be9a1f6c71fb254ba000de8cfe",
  "actions": [
    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "256908937f3cda8df522d5a3ba442eb935c3f11b"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
{
  "id": "376ba71113603004eae3c1b125c58cdc41d36b73",
  "actions": [
    {
      "type": "assign",
      "assignees": [
        "did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk"
      ]
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "3c849c9b555b18be9a1f6c71fb254ba000de8cfe"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
```

Conversely, if we provide only the `--until` option, the actions we get back
start from the beginning and stop at that revision:

```
$ rad cob actions --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --until 256908937f3cda8df522d5a3ba442eb935c3f11b
{
  "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
  "actions": [
    {
      "type": "comment",
      "body": "Flux capacitor power requirements exceed current supply"
    },
    {
      "type": "edit",
      "title": "flux capacitor underpowered"
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
{
  "id": "256908937f3cda8df522d5a3ba442eb935c3f11b",
  "actions": [
    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
```

Finally, if we provide both, we get back that exact range:

```
$ rad cob actions --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type xyz.radicle.issue --object d87dcfe8c2b3200e78b128d9b959cfdf7063fefe --from 256908937f3cda8df522d5a3ba442eb935c3f11b --until 3c849c9b555b18be9a1f6c71fb254ba000de8cfe
{
  "id": "256908937f3cda8df522d5a3ba442eb935c3f11b",
  "actions": [
    {
      "type": "comment.react",
      "id": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe",
      "reaction": "✨",
      "active": true
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
{
  "id": "3c849c9b555b18be9a1f6c71fb254ba000de8cfe",
  "actions": [
    {
      "type": "comment",
      "body": "Max power!",
      "replyTo": "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"
    }
  ],
  "author": "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
  "timestamp": 1671125284000,
  "parents": [
    "256908937f3cda8df522d5a3ba442eb935c3f11b"
  ],
  "related": [],
  "identity": "0656c217f917c3e06234771e9ecae53aba5e173e",
  "manifest": {
    "typeName": "xyz.radicle.issue",
    "version": 1
  }
}
```

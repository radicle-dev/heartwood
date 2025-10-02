Let's say we have a project with an issue created already. We can list all open issues.

```
$ rad issue list
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels             Assignees   Opened │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d87dcfe   flux capacitor underpowered   alice    (you)   good-first-issue               now    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

We can now assign ourselves to the open issue.

```
$ rad issue assign d87dcfe --add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --no-announce
```

It will now also show up in the list of issues assigned to us.

```
$ rad issue list --assigned me
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels             Assignees   Opened │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d87dcfe   flux capacitor underpowered   alice    (you)   good-first-issue   alice       now    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

If we now fix this issue, we can close it.

```
$ rad issue state --solved d87dcfe --no-announce
✓ Issue d87dcfe is now solved
```

It will not show up in the list of open issues anymore.

```
$ rad issue list
```

Instead, it will now show up in the list of solved issues.

```
$ rad issue list --solved
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels             Assignees   Opened │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d87dcfe   flux capacitor underpowered   alice    (you)   good-first-issue   alice       now    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note: You can achieve the same by omitting the `list` subcommand, since that's the fallback when no subcommand is specified.

```
$ rad issue --solved
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels             Assignees   Opened │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d87dcfe   flux capacitor underpowered   alice    (you)   good-first-issue   alice       now    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

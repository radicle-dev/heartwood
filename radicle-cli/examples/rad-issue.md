Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

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

Show the issue information issue.

```
$ rad issue show 0d18c61
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   0d18c610be2fbb4f47d45434c581f3bf0b0ff071        │
│ Author  alice (you)                                     │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```


Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be
labeled and assigned to others to work on. This is to ensure work is
not duplicated.

Let's assign ourselves to this one, this is to ensure work is not
duplicated. While we're at it, let's add a label.

```
$ rad issue assign 0d18c61 --add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --no-announce
$ rad issue label 0d18c61 --add good-first-issue --no-announce
```

It will now show in the list of issues assigned to us, along with the new label.

```
$ rad issue list --assigned
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels             Assignees   Opened │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   0d18c61   flux capacitor underpowered   alice    (you)   good-first-issue   alice       now    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad issue assign 0d18c61 --delete did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi --no-announce
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad issue comment 0d18c610be2fbb4f47d45434c581f3bf0b0ff071 --message 'The flux capacitor needs 1.21 Gigawatts' -q --no-announce
30d72f0b1e6a96e39c4c408369ea44186430d21b
$ rad issue comment 0d18c610be2fbb4f47d45434c581f3bf0b0ff071 --reply-to 30d72f0b1e6a96e39c4c408369ea44186430d21b --message 'More power!' -q --no-announce
4ff4fdc106a4fb3a2a035155e1fb3e8b3c1e0df4
```

We can see our comments by showing the issue:

```
$ rad issue show 0d18c610be2fbb4f47d45434c581f3bf0b0ff071
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   0d18c610be2fbb4f47d45434c581f3bf0b0ff071        │
│ Author  alice (you)                                     │
│ Labels  good-first-issue                                │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
├─────────────────────────────────────────────────────────┤
│ alice (you) now 30d72f0                                 │
│ The flux capacitor needs 1.21 Gigawatts                 │
├─────────────────────────────────────────────────────────┤
│ alice (you) now 4ff4fdc                                 │
│ More power!                                             │
╰─────────────────────────────────────────────────────────╯
```

We can also edit a comment:

```
$ rad issue comment 0d18c61 --edit 4ff4fdc -m "Even more power!"
╭─────────────────────────╮
│ alice (you) now 4ff4fdc │
│ Even more power!        │
╰─────────────────────────╯
```

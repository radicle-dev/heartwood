Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

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

Show the issue information issue.

```
$ rad issue show d185ee1
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61        │
│ Author  z6MknSL…StBU8Vi (you)                           │
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
$ rad issue assign d185ee1 --add did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
$ rad issue label d185ee1 --add good-first-issue
```

It will now show in the list of issues assigned to us, along with the new label.

```
$ rad issue list --assigned
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author                    Labels             Assignees         Opened │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   d185ee1   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)   good-first-issue   z6MknSL…StBU8Vi   now    │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad issue assign d185ee1 --delete did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad issue comment d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61 --message 'The flux capacitor needs 1.21 Gigawatts' -q
80ef590710edb64dfa57e8e940d6e4d0b0ae4217
$ rad issue comment d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61 --reply-to 80ef590710edb64dfa57e8e940d6e4d0b0ae4217 --message 'More power!' -q
91009820ca0996d93b9afd5739a4d2158a2ec898
```

We can see our comments by showing the issue:

```
$ rad issue show d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   d185ee16a00bac874c0bcbc2a8ad80fdce5e1e61        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Labels  good-first-issue                                │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
├─────────────────────────────────────────────────────────┤
│ z6MknSL…StBU8Vi (you) now 80ef590                       │
│ The flux capacitor needs 1.21 Gigawatts                 │
├─────────────────────────────────────────────────────────┤
│ z6MknSL…StBU8Vi (you) now 9100982                       │
│ More power!                                             │
╰─────────────────────────────────────────────────────────╯
```

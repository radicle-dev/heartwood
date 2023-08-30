Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   2f6eb49efac492327f71437b6bfc01b49afa0981        │
│ Author  bob (you)                                       │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

The issue is now listed under our project.

```
$ rad issue list
╭────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels   Assignees   Opened       │
├────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   2f6eb49   flux capacitor underpowered   bob      (you)                        [    ..    ] │
╰────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign this issue to ourself.

```
$ rad assign 2f6eb49efac492327f71437b6bfc01b49afa0981 --to did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
╭────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels   Assignees   Opened       │
├────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   2f6eb49   flux capacitor underpowered   bob      (you)            bob         [    ..    ] │
╰────────────────────────────────────────────────────────────────────────────────────────────────╯
```

In addition, you can see that when you run `rad issue show` you are listed under the `Assignees`.

```
$ rad issue show 2f6eb49
╭─────────────────────────────────────────────────────────╮
│ Title      flux capacitor underpowered                  │
│ Issue      2f6eb49efac492327f71437b6bfc01b49afa0981     │
│ Author     bob (you)                                    │
│ Assignees  z6Mkt67…v4N1tRk                              │
│ Status     open                                         │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

Note: this can always be undone with the `unassign` subcommand.

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment 2f6eb49efac492327f71437b6bfc01b49afa0981 --message 'The flux capacitor needs 1.21 Gigawatts'
24ab347afda760e77d565f9cb013c6db560f44fd
```

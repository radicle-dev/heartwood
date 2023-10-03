Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d        │
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
│ ●   9bf82c1   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)                        [    ..    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Show the issue information issue.

```
$ rad issue show 9bf82c1
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```


Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign ourselves to this one.

```
$ rad assign 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d --to did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author                    Labels   Assignees         Opened       │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   9bf82c1   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)            z6MknSL…StBU8Vi   [    ..    ] │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad unassign 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d --from did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad issue comment 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d --message 'The flux capacitor needs 1.21 Gigawatts' -q
1a8e9d3d62d22b247064b12d1d89ad8598504129
$ rad issue comment 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d --reply-to 1a8e9d3d62d22b247064b12d1d89ad8598504129 --message 'More power!' -q
fb6ab7e0ca5be3c34688bcae37d7302bb824decf
```

We can see our comments by showing the issue:

```
$ rad issue show 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
├─────────────────────────────────────────────────────────┤
│ z6MknSL…StBU8Vi (you) [   ...    ] 1a8e9d3              │
│ The flux capacitor needs 1.21 Gigawatts                 │
├─────────────────────────────────────────────────────────┤
│ z6MknSL…StBU8Vi (you) [   ...    ] fb6ab7e              │
│ More power!                                             │
╰─────────────────────────────────────────────────────────╯
```

Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   42028af21fabc09bfac2f25490f119f7c7e11542        │
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
│ ●   42028af   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)                        [    ..    ] │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Show the issue information issue.

```
$ rad issue show 42028af
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   42028af21fabc09bfac2f25490f119f7c7e11542        │
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
$ rad assign 42028af21fabc09bfac2f25490f119f7c7e11542 --to did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author                    Labels   Assignees         Opened       │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   42028af   flux capacitor underpowered   z6MknSL…StBU8Vi   (you)            z6MknSL…StBU8Vi   [    ..    ] │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad unassign 42028af21fabc09bfac2f25490f119f7c7e11542 --from did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment 42028af21fabc09bfac2f25490f119f7c7e11542 --message 'The flux capacitor needs 1.21 Gigawatts'
84492237dc0908b1e5b728d1a4e5f1343b6ffe9b
$ rad comment 42028af21fabc09bfac2f25490f119f7c7e11542 --reply-to 84492237dc0908b1e5b728d1a4e5f1343b6ffe9b --message 'More power!'
dd679552a15e2db73bbedf3084f5f7c62bb0d724
```

We can see our comments by showing the issue:

```
$ rad issue show 42028af21fabc09bfac2f25490f119f7c7e11542
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   42028af21fabc09bfac2f25490f119f7c7e11542        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
├─────────────────────────────────────────────────────────┤
│ z6MknSL…StBU8Vi (you) [   ...    ] 8449223              │
│ The flux capacitor needs 1.21 Gigawatts                 │
├─────────────────────────────────────────────────────────┤
│ z6MknSL…StBU8Vi (you) [   ...    ] dd67955              │
│ More power!                                             │
╰─────────────────────────────────────────────────────────╯
```

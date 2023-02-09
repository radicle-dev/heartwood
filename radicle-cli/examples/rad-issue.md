Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply"
```

The issue is now listed under our project.

```
$ rad issue list
2b4650e3c66d568132034de0d02871a2fbf9c5b5 "flux capacitor underpowered"
```

Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign ourselves to this one.

```
$ rad assign 2b4650e3c66d568132034de0d02871a2fbf9c5b5 z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
2b4650e3c66d568132034de0d02871a2fbf9c5b5 "flux capacitor underpowered" z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad unassign 2b4650e3c66d568132034de0d02871a2fbf9c5b5 z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment 2b4650e3c66d568132034de0d02871a2fbf9c5b5 --message 'The flux capacitor needs 1.21 Gigawatts'
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/7
$ rad comment 2b4650e3c66d568132034de0d02871a2fbf9c5b5 --reply-to z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/7 --message 'More power!'
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/8
```

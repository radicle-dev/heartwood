Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply"
```

The issue is now listed under our project.

```
$ rad issue list
17e52fd6b2cd2e0f9317e18371dc978a691ed1e3 "flux capacitor underpowered"
```

Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign ourselves to this one.

```
$ rad assign 17e52fd6b2cd2e0f9317e18371dc978a691ed1e3 z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
17e52fd6b2cd2e0f9317e18371dc978a691ed1e3 "flux capacitor underpowered" z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad unassign 17e52fd6b2cd2e0f9317e18371dc978a691ed1e3 z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment 17e52fd6b2cd2e0f9317e18371dc978a691ed1e3 --message 'The flux capacitor needs 1.21 Gigawatts'
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/6
$ rad comment 17e52fd6b2cd2e0f9317e18371dc978a691ed1e3 --reply-to z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/6 --message 'More power!'
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/7
```

Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
```

The issue is now listed under our project.

```
$ rad issue list
e8eb9ca4afa050499b259842ddef2d41abf0fd83 "flux capacitor underpowered"
```

Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign ourselves to this one.

```
$ rad assign e8eb9ca4afa050499b259842ddef2d41abf0fd83 did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
e8eb9ca4afa050499b259842ddef2d41abf0fd83 "flux capacitor underpowered" did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad unassign e8eb9ca4afa050499b259842ddef2d41abf0fd83 did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment e8eb9ca4afa050499b259842ddef2d41abf0fd83 --message 'The flux capacitor needs 1.21 Gigawatts'
f1895792f7b1b56590aa21e34454bde74d04649a
$ rad comment e8eb9ca4afa050499b259842ddef2d41abf0fd83 --reply-to f1895792f7b1b56590aa21e34454bde74d04649a --message 'More power!'
0bf5f874c57ac0a5cc010a9895dd0fec9edc4f3d
```

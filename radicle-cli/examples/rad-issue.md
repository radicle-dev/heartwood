Project 'todo' items are called 'issue's.  They can be inspected and modified
using the 'issue' subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
title: flux capacitor underpowered
state: open
tags: []
assignees: []

Flux capacitor power requirements exceed current supply
```

The issue is now listed under our project.

```
$ rad issue list
2e8c1bf3fe0532a314778357c886608a966a34bd "flux capacitor underpowered"
```

Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign ourselves to this one.

```
$ rad assign 2e8c1bf3fe0532a314778357c886608a966a34bd did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
2e8c1bf3fe0532a314778357c886608a966a34bd "flux capacitor underpowered" did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Note: this can always be undone with the `unassign` subcommand.

```
$ rad unassign 2e8c1bf3fe0532a314778357c886608a966a34bd did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment 2e8c1bf3fe0532a314778357c886608a966a34bd --message 'The flux capacitor needs 1.21 Gigawatts'
9822748bd076595a2408aad02b3a0d9f94fec7e0
$ rad comment 2e8c1bf3fe0532a314778357c886608a966a34bd --reply-to 9822748bd076595a2408aad02b3a0d9f94fec7e0 --message 'More power!'
edec8d07bf3788b98943394c1274910b8f12d35c
```

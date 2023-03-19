Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

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
b05e945bb63c11bf80320f4e26ad1d1f7c51f755 "flux capacitor underpowered" ❲unassigned❳
```

Great! Now we've documented the issue for ourselves and others.

Just like with other project management systems, the issue can be assigned to
others to work on.  This is to ensure work is not duplicated.

Let's assign this issue to ourself.

```
$ rad assign b05e945bb63c11bf80320f4e26ad1d1f7c51f755 --to did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

It will now show in the list of issues assigned to us.

```
$ rad issue list --assigned
b05e945bb63c11bf80320f4e26ad1d1f7c51f755 "flux capacitor underpowered" did:key:z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

Note: this can always be undone with the `unassign` subcommand.

Great, now we have communicated to the world about our car's defect.

But wait! We've found an important detail about the car's power requirements.
It will help whoever works on a fix.

```
$ rad comment b05e945bb63c11bf80320f4e26ad1d1f7c51f755 --message 'The flux capacitor needs 1.21 Gigawatts'
8b9ee0f0a530f0318e100ea8b9ed3a723bd584f6
```

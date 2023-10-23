Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   d457c205de780d37d1c16efbe5333aed4662a6c3        │
│ Author  bob (you)                                       │
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
│ ●   d457c20   flux capacitor underpowered   bob      (you)                        now    │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

Great! Now we've documented the issue for ourselves and others. But wait, we've
found an important detail about the car's power requirements. It will help
whoever works on a fix.

```
$ rad issue comment d457c205de780d37d1c16efbe5333aed4662a6c3 --message 'The flux capacitor needs 1.21 Gigawatts' -q
2e8b9538aedaea418e90e90d19e61edf3f022933
```

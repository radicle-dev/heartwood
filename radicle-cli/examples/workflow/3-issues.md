Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   b0c5579944e37d0bf3455bf1cec9c91596c6f47a        │
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
│ ●   b0c5579   flux capacitor underpowered   bob      (you)                        now    │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

Great! Now we've documented the issue for ourselves and others. But wait, we've
found an important detail about the car's power requirements. It will help
whoever works on a fix.

```
$ rad issue comment b0c5579944e37d0bf3455bf1cec9c91596c6f47a --message 'The flux capacitor needs 1.21 Gigawatts' -q
c45d2acdd1de992c0127fd44519b8a5260695497
```

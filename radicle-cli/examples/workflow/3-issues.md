Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   3b2f7e674bc39d5ff93abf2c68d8233fa4aa8806        │
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
│ ●   3b2f7e6   flux capacitor underpowered   bob      (you)                        now    │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

Great! Now we've documented the issue for ourselves and others. But wait, we've
found an important detail about the car's power requirements. It will help
whoever works on a fix.

```
$ rad issue comment 3b2f7e674bc39d5ff93abf2c68d8233fa4aa8806 --message 'The flux capacitor needs 1.21 Gigawatts' -q --no-announce
3a4261f49c44a5832c7f18179d00cffa0deb17f9
```

Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9037b7a42323d4b79e6a48b7d05d3bbaae11d69b        │
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
│ ●   9037b7a   flux capacitor underpowered   bob      (you)                        now    │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

Great! Now we've documented the issue for ourselves and others. But wait, we've
found an important detail about the car's power requirements. It will help
whoever works on a fix.

```
$ rad issue comment 9037b7a42323d4b79e6a48b7d05d3bbaae11d69b --message 'The flux capacitor needs 1.21 Gigawatts' -q
400cb155f512b4880858bb05f935104c34167b28
```

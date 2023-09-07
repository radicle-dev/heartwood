Project "todos" are called *issues*.  They can be inspected and
modified using the `issue` subcommand.

Let's say the new car you are designing with your peers has a problem with its flux capacitor.

```
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   2f6eb49efac492327f71437b6bfc01b49afa0981        │
│ Author  bob (you)                                       │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

The issue is now listed under our project.

```
$ rad issue list
╭────────────────────────────────────────────────────────────────────────────────────────────────╮
│ ●   ID        Title                         Author           Labels   Assignees   Opened       │
├────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ●   2f6eb49   flux capacitor underpowered   bob      (you)                        [    ..    ] │
╰────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Great! Now we've documented the issue for ourselves and others. But wait, we've
found an important detail about the car's power requirements. It will help
whoever works on a fix.

```
$ rad comment 2f6eb49efac492327f71437b6bfc01b49afa0981 --message 'The flux capacitor needs 1.21 Gigawatts'
d2b50873009b93680698aef4f57f43f7e850b651
```

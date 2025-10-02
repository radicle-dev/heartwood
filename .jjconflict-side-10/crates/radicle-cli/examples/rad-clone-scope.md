By default `rad clone` should add a seeding policy with the `followed` scope:

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
✓ Creating checkout in ./heartwood..
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ rad seed
╭───────────────────────────────────────────────────────────────────╮
│ Repository                          Name        Policy   Scope    │
├───────────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   heartwood   allow    followed │
╰───────────────────────────────────────────────────────────────────╯
$ rm -rf heartwood
```

Specifying a different scope explicitly should update the policy:

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope all
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
✓ Creating checkout in ./heartwood..
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ rad seed
╭────────────────────────────────────────────────────────────────╮
│ Repository                          Name        Policy   Scope │
├────────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   heartwood   allow    all   │
╰────────────────────────────────────────────────────────────────╯
$ rm -rf heartwood
```

Running `rad clone` again without an explicit scope parameter should not change the existing policy:

```
$ rad clone rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Creating checkout in ./heartwood..
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ rad seed
╭────────────────────────────────────────────────────────────────╮
│ Repository                          Name        Policy   Scope │
├────────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji   heartwood   allow    all   │
╰────────────────────────────────────────────────────────────────╯
```

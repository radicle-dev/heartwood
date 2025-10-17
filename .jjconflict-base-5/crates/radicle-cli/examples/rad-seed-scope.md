By default `rad seed` should add a seeding policy with the `followed` scope:

```
$ rad seed --no-fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'followed'
$ rad seed
╭──────────────────────────────────────────────────────────────╮
│ Repository                          Name   Policy   Scope    │
├──────────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji          allow    followed │
╰──────────────────────────────────────────────────────────────╯
```

The policy can be updated by explicitly specifying a different scope:

```
$ rad seed --no-fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --scope all
✓ Seeding policy updated for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
$ rad seed
╭───────────────────────────────────────────────────────────╮
│ Repository                          Name   Policy   Scope │
├───────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji          allow    all   │
╰───────────────────────────────────────────────────────────╯
```

Running `rad seed` again without an explicit scope parameter should not change the existing policy:

```
$ rad seed --no-fetch rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Seeding policy exists for rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji with scope 'all'
$ rad seed
╭───────────────────────────────────────────────────────────╮
│ Repository                          Name   Policy   Scope │
├───────────────────────────────────────────────────────────┤
│ rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji          allow    all   │
╰───────────────────────────────────────────────────────────╯
```

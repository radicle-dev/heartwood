Labeling an issue is easy, let's add the `bug` and `good-first-issue` labels to
some issue:

```
$ rad label 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d bug good-first-issue
```

We can now show the issue to check whether those labels were added:

```
$ rad issue show 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d --format header
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Labels  bug, good-first-issue                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

Untagging an issue is very similar:

```
$ rad unlabel 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d good-first-issue
```

Notice that the `good-first-issue` label has disappeared:

```
$ rad issue show 9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d --format header
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   9bf82c141d5a9c54bb1d6b4517eb3bb7da8fb30d        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Labels  bug                                             │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

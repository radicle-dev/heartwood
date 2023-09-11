Labeling an issue is easy, let's add the `bug` and `good-first-issue` labels to
some issue:

```
$ rad label 42028af21fabc09bfac2f25490f119f7c7e11542 bug good-first-issue
```

We can now show the issue to check whether those labels were added:

```
$ rad issue show 42028af21fabc09bfac2f25490f119f7c7e11542 --format header
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   42028af21fabc09bfac2f25490f119f7c7e11542        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Labels  bug, good-first-issue                           │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

Untagging an issue is very similar:

```
$ rad unlabel 42028af21fabc09bfac2f25490f119f7c7e11542 good-first-issue
```

Notice that the `good-first-issue` label has disappeared:

```
$ rad issue show 42028af21fabc09bfac2f25490f119f7c7e11542 --format header
╭─────────────────────────────────────────────────────────╮
│ Title   flux capacitor underpowered                     │
│ Issue   42028af21fabc09bfac2f25490f119f7c7e11542        │
│ Author  z6MknSL…StBU8Vi (you)                           │
│ Labels  bug                                             │
│ Status  open                                            │
│                                                         │
│ Flux capacitor power requirements exceed current supply │
╰─────────────────────────────────────────────────────────╯
```

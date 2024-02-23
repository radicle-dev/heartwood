Let's explore the `rad patch update` plumbing command. First we create a patch:

``` (stderr)
$ git checkout -q -b feature/1
$ git commit -q -m "Not a real change" --allow-empty
```
``` (stderr)
$ git push rad HEAD:refs/patches
✓ Patch b6a23eb08656de0ef1fcc0b5fe8820841e5cb2e5 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ rad patch show b6a23eb08656de0ef1fcc0b5fe8820841e5cb2e5
╭────────────────────────────────────────────────────╮
│ Title     Not a real change                        │
│ Patch     b6a23eb08656de0ef1fcc0b5fe8820841e5cb2e5 │
│ Author    z6MknSL…StBU8Vi (you)                    │
│ Head      51b2f0f77b9849bfaa3e9d3ff68ee2f57771d20c │
│ Branches  feature/1                                │
│ Commits   ahead 1, behind 0                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 51b2f0f Not a real change                          │
├────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (51b2f0f) now    │
╰────────────────────────────────────────────────────╯
```

We can make some changes to the repository:

```
$ git mv README README.md
$ git commit -q -m "Rename readme file"
```

Let's push the changes, but not to the magic ref, that way the push doesn't
update our patch:

```
$ git push rad HEAD:refs/heads/feature/1
```

Now, instead of using `git push` to update the patch, as we normally would,
we run:

```
$ rad patch update b6a23eb08656de0ef1fcc0b5fe8820841e5cb2e5 -m "Updated patch" --no-announce
ea7def3857f62f404606d7cd6490cd0de4eaebd1
```

The command outputs the new Revision ID, which we can now see here:

```
$ rad patch show b6a23eb08656de0ef1fcc0b5fe8820841e5cb2e5
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Not a real change                                         │
│ Patch     b6a23eb08656de0ef1fcc0b5fe8820841e5cb2e5                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      4d272148458a17620541555b1f0905c01658aa9f                  │
│ Branches  feature/1                                                 │
│ Commits   ahead 2, behind 0                                         │
│ Status    open                                                      │
├─────────────────────────────────────────────────────────────────────┤
│ 4d27214 Rename readme file                                          │
│ 51b2f0f Not a real change                                           │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (51b2f0f) now                     │
│ ↑ updated to ea7def3857f62f404606d7cd6490cd0de4eaebd1 (4d27214) now │
╰─────────────────────────────────────────────────────────────────────╯
```

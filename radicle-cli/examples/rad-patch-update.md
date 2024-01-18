Let's explore the `rad patch update` plumbing command. First we create a patch:

``` (stderr)
$ git checkout -q -b feature/1
$ git commit -q -m "Not a real change" --allow-empty
```
``` (stderr)
$ git push rad HEAD:refs/patches
✓ Patch bbe51b0a6b0edb6836eec7ca6fe9e8e918b05954 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

```
$ rad patch show bbe51b0a6b0edb6836eec7ca6fe9e8e918b05954
╭────────────────────────────────────────────────────╮
│ Title     Not a real change                        │
│ Patch     bbe51b0a6b0edb6836eec7ca6fe9e8e918b05954 │
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
$ rad patch update bbe51b0a6b0edb6836eec7ca6fe9e8e918b05954 -m "Updated patch" --no-announce
c5fa2b805fa9dd61c999a810c35d39081b586472
```

The command outputs the new Revision ID, which we can now see here:

```
$ rad patch show bbe51b0a6b0edb6836eec7ca6fe9e8e918b05954
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Not a real change                                         │
│ Patch     bbe51b0a6b0edb6836eec7ca6fe9e8e918b05954                  │
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
│ ↑ updated to c5fa2b805fa9dd61c999a810c35d39081b586472 (4d27214) now │
╰─────────────────────────────────────────────────────────────────────╯
```

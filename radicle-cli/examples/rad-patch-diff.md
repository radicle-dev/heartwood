Using `rad patch diff`, we can output the patch diff:

``` ./README.md
Hello World!
```
```
$ git checkout -b feature/1
$ git add README.md
$ git commit -m "Add README" -q
$ git push rad HEAD:refs/patches
```
```
$ rad patch diff 147309e
╭───────────────────────────╮
│ README.md +1 ❲created❳    │
├───────────────────────────┤
│ @@ -0,0 +1,1 @@           │
│      1     + Hello World! │
╰───────────────────────────╯

```

If we add another file and update the patch, we can see it in the diff.

``` ./RADICLE.md
Hello Radicle!
```
```
$ git add RADICLE.md
$ git commit --amend -q
$ git push -f
```
```
$ rad patch diff 147309e
╭─────────────────────────────╮
│ RADICLE.md +1 ❲created❳     │
├─────────────────────────────┤
│ @@ -0,0 +1,1 @@             │
│      1     + Hello Radicle! │
╰─────────────────────────────╯

╭─────────────────────────────╮
│ README.md +1 ❲created❳      │
├─────────────────────────────┤
│ @@ -0,0 +1,1 @@             │
│      1     + Hello World!   │
╰─────────────────────────────╯

```

Buf if we only want to see the changes from the first revision, we can do that
too.

```
$ rad patch diff 147309e --revision 147309e
╭───────────────────────────╮
│ README.md +1 ❲created❳    │
├───────────────────────────┤
│ @@ -0,0 +1,1 @@           │
│      1     + Hello World! │
╰───────────────────────────╯

```

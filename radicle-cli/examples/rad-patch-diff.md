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
$ rad patch
╭──────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title       Author         Reviews  Head     +   -   Updated │
├──────────────────────────────────────────────────────────────────────────┤
│ ●  a44b0da  Add README  alice   (you)  -        2420bc3  +1  -0  now     │
╰──────────────────────────────────────────────────────────────────────────╯
$ rad patch diff a44b0da
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
$ rad patch diff a44b0da
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
$ rad patch diff a44b0da --revision a44b0da
╭───────────────────────────╮
│ README.md +1 ❲created❳    │
├───────────────────────────┤
│ @@ -0,0 +1,1 @@           │
│      1     + Hello World! │
╰───────────────────────────╯

```

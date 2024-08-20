Let's start by creating some files we will patch:

``` ./MENU.txt
Classics
--------
Salmon Tartare
Mac & Cheese

Comfort Food
------------
Reuben Sandwich
Club Sandwich
Fried Shrimp Basket

Sides
-----
French Fries
```

``` ./INSTRUCTIONS.txt
Notes on how to prepare food go here.
```

``` ./.gitignore
*.draft
```

Now let's commit and push them:

```
$ git add MENU.txt INSTRUCTIONS.txt .gitignore
$ git commit -q -a -m "Add files"
$ git push rad master
```

We can now make some changes and create a patch:

```
$ sed -i '$a Garlic Green Beans' MENU.txt
$ sed -i '3i\Baked Brie' MENU.txt
$ sed -i 's/French Fries/French Fries!/' MENU.txt
$ rm .gitignore
$ mkdir notes
$ mv INSTRUCTIONS.txt notes/
```

``` ./DISCLAIMER.txt
All food is served as-is, with no warranty!
```

```
$ git checkout -q -b patch/1
$ git add .
$ git status --short
D  .gitignore
A  DISCLAIMER.txt
M  MENU.txt
R  INSTRUCTIONS.txt -> notes/INSTRUCTIONS.txt
$ git commit -q -m "Update files"
```

``` (stderr)
$ git push rad HEAD:refs/patches
✓ Patch d34084970fdd4de9d8125165f5ac39ac70d3806c opened
To rad://z3W5xAVWJ9Gc4LbN16mE3tjWX92t2/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Finally, we do a review of the patch by hunk. The output of this command should
match `git diff master -W100% -U5 --patience`:

```
$ rad patch review --patch -U5 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
╭──────────────────────╮
│ .gitignore ❲deleted❳ │
├──────────────────────┤
│ @@ -1,1 +0,0 @@      │
│ 1          - *.draft │
╰──────────────────────╯
╭──────────────────────────────────────────────────────────╮
│ DISCLAIMER.txt ❲created❳                                 │
├──────────────────────────────────────────────────────────┤
│ @@ -0,0 +1,1 @@                                          │
│      1     + All food is served as-is, with no warranty! │
╰──────────────────────────────────────────────────────────╯
╭─────────────────────────────╮
│ MENU.txt                    │
├─────────────────────────────┤
│ @@ -1,7 +1,8 @@             │
│ 1    1       Classics       │
│ 2    2       --------       │
│      3     + Baked Brie     │
│ 3    4       Salmon Tartare │
│ 4    5       Mac & Cheese   │
│ 5    6                      │
│ 6    7       Comfort Food   │
│ 7    8       ------------   │
╰─────────────────────────────╯
╭──────────────────────────────────╮
│ MENU.txt                         │
├──────────────────────────────────┤
│ @@ -9,6 +10,7 @@ Reuben Sandwich │
│ 9    10      Club Sandwich       │
│ 10   11      Fried Shrimp Basket │
│ 11   12                          │
│ 12   13      Sides               │
│ 13   14      -----               │
│ 14         - French Fries        │
│      15    + French Fries!       │
│      16    + Garlic Green Beans  │
╰──────────────────────────────────╯
╭────────────────────────────────────────────────────╮
│ INSTRUCTIONS.txt -> notes/INSTRUCTIONS.txt ❲moved❳ │
╰────────────────────────────────────────────────────╯
```

Now let's accept these hunks one by one..

```
$ rad patch review --patch --accept --hunk 1 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
✓ Loaded existing review ([..]) for patch d34084970fdd4de9d8125165f5ac39ac70d3806c
╭──────────────────────╮
│ .gitignore ❲deleted❳ │
├──────────────────────┤
│ @@ -1,1 +0,0 @@      │
│ 1          - *.draft │
╰──────────────────────╯
✓ Updated brain to a5fccf0e977225ff13c3f74c43faf4cb679bf835
```
```
$ rad patch review --patch --accept --hunk 1 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
✓ Loaded existing review ([..]) for patch d34084970fdd4de9d8125165f5ac39ac70d3806c
╭──────────────────────────────────────────────────────────╮
│ DISCLAIMER.txt ❲created❳                                 │
├──────────────────────────────────────────────────────────┤
│ @@ -0,0 +1,1 @@                                          │
│      1     + All food is served as-is, with no warranty! │
╰──────────────────────────────────────────────────────────╯
✓ Updated brain to 2cdb82ea726e64d3b52847c7699d0d4759198f5c
```
```
$ rad patch review --patch --accept -U3 --hunk 1 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
✓ Loaded existing review ([..]) for patch d34084970fdd4de9d8125165f5ac39ac70d3806c
╭─────────────────────────────╮
│ MENU.txt                    │
├─────────────────────────────┤
│ @@ -1,5 +1,6 @@             │
│ 1    1       Classics       │
│ 2    2       --------       │
│      3     + Baked Brie     │
│ 3    4       Salmon Tartare │
│ 4    5       Mac & Cheese   │
│ 5    6                      │
╰─────────────────────────────╯
✓ Updated brain to d4aecbb859a802a3215def0b538358bf63593953
```
```
$ rad patch review --patch --accept -U3 --hunk 1 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
✓ Loaded existing review ([..]) for patch d34084970fdd4de9d8125165f5ac39ac70d3806c
╭───────────────────────────────────────╮
│ MENU.txt                              │
├───────────────────────────────────────┤
│ @@ -12,4 +12,5 @@ Fried Shrimp Basket │
│ 12   12                               │
│ 13   13      Sides                    │
│ 14   14      -----                    │
│ 15         - French Fries             │
│      15    + French Fries!            │
│      16    + Garlic Green Beans       │
╰───────────────────────────────────────╯
✓ Updated brain to 59cee720b0642b1491b241400912b35926a76c3f
```

```
$ rad patch review --patch --accept --hunk 1 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
✓ Loaded existing review ([..]) for patch d34084970fdd4de9d8125165f5ac39ac70d3806c
╭────────────────────────────────────────────────────╮
│ INSTRUCTIONS.txt -> notes/INSTRUCTIONS.txt ❲moved❳ │
╰────────────────────────────────────────────────────╯
✓ Updated brain to 3effc8f6462fa2573697072245e57708c4dcbe62
```

```
$ rad patch review --patch --accept --hunk 1 d34084970fdd4de9d8125165f5ac39ac70d3806c --no-announce
✓ Loaded existing review ([..]) for patch d34084970fdd4de9d8125165f5ac39ac70d3806c
✓ All hunks have been reviewed
```

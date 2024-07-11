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
✓ Patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

Finally, we do a review of the patch by hunk. The output of this command should
match `git diff master -W100% -U5 --patience`:

```
$ rad patch review --patch -U5 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
diff --git a/.gitignore b/.gitignore
deleted file mode 100644
index 7937fb3..0000000
--- a/.gitignore
+++ /dev/null
@@ -1 +0,0 @@
-*.draft
diff --git a/DISCLAIMER.txt b/DISCLAIMER.txt
new file mode 100644
index 0000000..2b5bd86
--- /dev/null
+++ b/DISCLAIMER.txt
@@ -0,0 +1 @@
+All food is served as-is, with no warranty!
diff --git a/MENU.txt b/MENU.txt
index 867958c..3af9741 100644
--- a/MENU.txt
+++ b/MENU.txt
@@ -1,7 +1,8 @@
 Classics
 --------
+Baked Brie
 Salmon Tartare
 Mac & Cheese
[..]
 Comfort Food
 ------------
@@ -9,6 +10,7 @@ Reuben Sandwich
 Club Sandwich
 Fried Shrimp Basket
[..]
 Sides
 -----
-French Fries
+French Fries!
+Garlic Green Beans
diff --git a/INSTRUCTIONS.txt b/notes/INSTRUCTIONS.txt
similarity index 100%
rename from INSTRUCTIONS.txt
rename to notes/INSTRUCTIONS.txt
```

Now let's accept these hunks one by one..

```
$ rad patch review --patch --accept --hunk 1 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
✓ Loaded existing review ([..]) for patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23
diff --git a/.gitignore b/.gitignore
deleted file mode 100644
index 7937fb3..0000000
--- a/.gitignore
+++ /dev/null
@@ -1 +0,0 @@
-*.draft
✓ Updated review tree to a5fccf0e977225ff13c3f74c43faf4cb679bf835
```
```
$ rad patch review --patch --accept --hunk 1 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
✓ Loaded existing review ([..]) for patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23
diff --git a/DISCLAIMER.txt b/DISCLAIMER.txt
new file mode 100644
index 0000000..2b5bd86
--- /dev/null
+++ b/DISCLAIMER.txt
@@ -0,0 +1 @@
+All food is served as-is, with no warranty!
✓ Updated review tree to 2cdb82ea726e64d3b52847c7699d0d4759198f5c
```
```
$ rad patch review --patch --accept -U3 --hunk 1 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
✓ Loaded existing review ([..]) for patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23
diff --git a/MENU.txt b/MENU.txt
index 867958c..3af9741 100644
--- a/MENU.txt
+++ b/MENU.txt
@@ -1,5 +1,6 @@
 Classics
 --------
+Baked Brie
 Salmon Tartare
 Mac & Cheese
[..]
✓ Updated review tree to d4aecbb859a802a3215def0b538358bf63593953
```
```
$ rad patch review --patch --accept -U3 --hunk 1 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
✓ Loaded existing review ([..]) for patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23
diff --git a/MENU.txt b/MENU.txt
index 4e2e828..3af9741 100644
--- a/MENU.txt
+++ b/MENU.txt
@@ -12,4 +12,5 @@ Fried Shrimp Basket
[..]
 Sides
 -----
-French Fries
+French Fries!
+Garlic Green Beans
✓ Updated review tree to 59cee720b0642b1491b241400912b35926a76c3f
```

```
$ rad patch review --patch --accept --hunk 1 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
✓ Loaded existing review ([..]) for patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23
diff --git a/INSTRUCTIONS.txt b/notes/INSTRUCTIONS.txt
similarity index 100%
rename from INSTRUCTIONS.txt
rename to notes/INSTRUCTIONS.txt
✓ Updated review tree to 3effc8f6462fa2573697072245e57708c4dcbe62
```

```
$ rad patch review --patch --accept --hunk 1 7a2ac7e2841cc1e7394f99f107555a499b1d3f23 --no-announce
✓ Loaded existing review ([..]) for patch 7a2ac7e2841cc1e7394f99f107555a499b1d3f23
✓ All hunks have been reviewed
```

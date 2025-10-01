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
diff --git a/README.md b/README.md
new file mode 100644
index 0000000..980a0d5
--- /dev/null
+++ b/README.md
@@ -0,0 +1 @@
+Hello World!
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
diff --git a/RADICLE.md b/RADICLE.md
new file mode 100644
index 0000000..e517184
--- /dev/null
+++ b/RADICLE.md
@@ -0,0 +1 @@
+Hello Radicle!
diff --git a/README.md b/README.md
new file mode 100644
index 0000000..980a0d5
--- /dev/null
+++ b/README.md
@@ -0,0 +1 @@
+Hello World!
```

Buf if we only want to see the changes from the first revision, we can do that
too.

```
$ rad patch diff 147309e --revision 147309e
diff --git a/README.md b/README.md
new file mode 100644
index 0000000..980a0d5
--- /dev/null
+++ b/README.md
@@ -0,0 +1 @@
+Hello World!
```

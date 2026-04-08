``` (stderr)
$ rad diff
! Deprecated: The command/option `rad diff` is deprecated and will be removed. Please use `git diff` instead.
```

Exploring `rad diff`.

``` ./main.c
#include <stdio.h>

int main(void) {
    printf("Hello World!\n");
    return 0;
}
```

```
$ ls
README
main.c
```

```
$ git mv README README.md
$ git add main.c
$ git commit -m "Make changes"
[master 5f771e0] Make changes
 2 files changed, 6 insertions(+)
 rename README => README.md (100%)
 create mode 100644 main.c
```

```
$ rad diff HEAD^ HEAD
diff --git a/README b/README.md
similarity index 100%
rename from README
rename to README.md
diff --git a/main.c b/main.c
new file mode 100644
index 0000000..aae4e0e
--- /dev/null
+++ b/main.c
@@ -0,0 +1,6 @@
+#include <stdio.h>
+
+int main(void) {
+    printf("Hello World!/n");
+    return 0;
+}
```

```
$ sed -i 's/Hello World/Hello Radicle/' main.c
$ rad diff
diff --git a/main.c b/main.c
index aae4e0e..a3ed869 100644
--- a/main.c
+++ b/main.c
@@ -1,6 +1,6 @@
 #include <stdio.h>
 
 int main(void) {
-    printf("Hello World!/n");
+    printf("Hello Radicle!/n");
     return 0;
 }
```

```
$ git add main.c
$ rad diff
$ rad diff --staged
diff --git a/main.c b/main.c
index aae4e0e..a3ed869 100644
--- a/main.c
+++ b/main.c
@@ -1,6 +1,6 @@
 #include <stdio.h>
 
 int main(void) {
-    printf("Hello World!/n");
+    printf("Hello Radicle!/n");
     return 0;
 }
```

```
$ git rm -f -q main.c
$ rad diff --staged
diff --git a/main.c b/main.c
deleted file mode 100644
index aae4e0e..0000000
--- a/main.c
+++ /dev/null
@@ -1,6 +0,0 @@
-#include <stdio.h>
-
-int main(void) {
-    printf("Hello World!/n");
-    return 0;
-}
```

For now, copies are not detected.

```
$ git reset --hard master -q
$ mkdir docs
$ cp README.md docs/README.md
$ git add docs
$ rad diff --staged
diff --git a/docs/README.md b/docs/README.md
new file mode 100644
index 0000000..980a0d5
--- /dev/null
+++ b/docs/README.md
@@ -0,0 +1 @@
+Hello World!
$ git reset
$ git checkout .
```

Empty file.

```
$ touch EMPTY
$ git add EMPTY
$ rad diff --staged
diff --git a/EMPTY b/EMPTY
new file mode 100644
index 0000000..e69de29
$ git reset
$ git checkout .
```

File mode change.

```
$ chmod +x README.md
$ rad diff
diff --git a/README.md b/README.md
old mode 100644
new mode 100755
$ git reset -q
$ git checkout .
```

Binary file.

```
$ touch file.bin
$ truncate -s 8 file.bin
$ git add file.bin
$ rad diff --staged
diff --git a/file.bin b/file.bin
new file mode 100644
index 0000000..1b1cb4d
Binary files /dev/null and b/file.bin differ
```

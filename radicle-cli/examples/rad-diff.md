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
╭────────────────────────────────────────────╮
│ README -> README.md ❲moved❳                │
╰────────────────────────────────────────────╯

╭────────────────────────────────────────────╮
│ main.c +6 ❲created❳                        │
├────────────────────────────────────────────┤
│ @@ -0,0 +1,6 @@                            │
│      1     + #include <stdio.h>            │
│      2     +                               │
│      3     + int main(void) {              │
│      4     +     printf("Hello World!/n"); │
│      5     +     return 0;                 │
│      6     + }                             │
╰────────────────────────────────────────────╯

```

```
$ sed -i 's/Hello World/Hello Radicle/' main.c
$ rad diff
╭──────────────────────────────────────────────╮
│ main.c -1 +1                                 │
├──────────────────────────────────────────────┤
│ @@ -1,6 +1,6 @@                              │
│ 1    1       #include <stdio.h>              │
│ 2    2                                       │
│ 3    3       int main(void) {                │
│ 4          -     printf("Hello World!/n");   │
│      4     +     printf("Hello Radicle!/n"); │
│ 5    5           return 0;                   │
│ 6    6       }                               │
╰──────────────────────────────────────────────╯

```

```
$ git add main.c
$ rad diff
$ rad diff --staged
╭──────────────────────────────────────────────╮
│ main.c -1 +1                                 │
├──────────────────────────────────────────────┤
│ @@ -1,6 +1,6 @@                              │
│ 1    1       #include <stdio.h>              │
│ 2    2                                       │
│ 3    3       int main(void) {                │
│ 4          -     printf("Hello World!/n");   │
│      4     +     printf("Hello Radicle!/n"); │
│ 5    5           return 0;                   │
│ 6    6       }                               │
╰──────────────────────────────────────────────╯

```

```
$ git rm -f -q main.c
$ rad diff --staged
╭────────────────────────────────────────────╮
│ main.c -6 ❲deleted❳                        │
├────────────────────────────────────────────┤
│ @@ -1,6 +0,0 @@                            │
│ 1          - #include <stdio.h>            │
│ 2          -                               │
│ 3          - int main(void) {              │
│ 4          -     printf("Hello World!/n"); │
│ 5          -     return 0;                 │
│ 6          - }                             │
╰────────────────────────────────────────────╯

```

For now, copies are not detected.

```
$ git reset --hard master -q
$ mkdir docs
$ cp README.md docs/README.md
$ git add docs
$ rad diff --staged
╭─────────────────────────────╮
│ docs/README.md +1 ❲created❳ │
├─────────────────────────────┤
│ @@ -0,0 +1,1 @@             │
│      1     + Hello World!   │
╰─────────────────────────────╯

$ git reset
$ git checkout .
```

Empty file.

```
$ touch EMPTY
$ git add EMPTY
$ rad diff --staged
╭─────────────────╮
│ EMPTY ❲created❳ │
╰─────────────────╯

$ git reset
$ git checkout .
```

File mode change.

```
$ chmod +x README.md
$ rad diff
╭───────────────────────────────────────────╮
│ README.md 100644 -> 100755 ❲mode changed❳ │
╰───────────────────────────────────────────╯

$ git reset -q
$ git checkout .
```

Binary file.

```
$ touch file.bin
$ truncate -s 8 file.bin
$ git add file.bin
$ rad diff --staged
╭─────────────────────────────╮
│ file.bin ❲binary❳ ❲created❳ │
╰─────────────────────────────╯

```

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

In this example, we explore how the `ahead/behind` indicator works, and what is
shown as diffs in the case of divergent branches.

First we add the `CONTRIBUTORS` file to `master`, which contains one entry:
```
$ git checkout -q master
$ git add CONTRIBUTORS
$ git commit -a -q -m "Add contributors"
$ git push rad master
$ cat CONTRIBUTORS
Alice Jones
```

Then we create a feature branch which adds another entry:
```
$ git checkout -q -b feature/1
$ sed -i '$a Alan K' CONTRIBUTORS
$ git commit -a -q -m "Add Alan"
```

We go back to master, and add a different second entry, essentially forking
the history:
```
$ git checkout -q master
$ sed -i '$a Jason Bourne' CONTRIBUTORS
$ git commit -a -q -m "Add Jason"
$ git push rad master
$ git log --graph --decorate --abbrev-commit --pretty=oneline --all
* d60e9c1 (feature/1) Add Alan
| * 79b6b56 (HEAD -> master, rad/master) Add Jason
|/ [..]
* 046704f Add contributors
* 4c66f0e Second commit
* 60d31e8 Initial commit
```

Then we create a patch from `feature/1`:
``` (stderr)
$ git push rad feature/1:refs/patches
✓ Patch d2aee03ab1a0b2f54f63a4db54acfa359fc86bb6 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   feature/1 -> refs/patches
```

When listing, we see that it has one addition:
```
$ rad patch list
╭────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title     Author         Reviews  Head     +   -   Updated  Labels │
├────────────────────────────────────────────────────────────────────────────────┤
│ ●  d2aee03  Add Alan  alice   (you)  -        d60e9c1  +1  -0  now             │
╰────────────────────────────────────────────────────────────────────────────────╯
```

When showing the patch, we see that it is `ahead 1, behind 1`, since master has
diverged by one commit:
```
$ rad patch show -v -p d2aee03
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add Alan                                                                                                                                                   │
│ Patch     d2aee03ab1a0b2f54f63a4db54acfa359fc86bb6                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      d60e9c18c50bd5b74df1fadccb340f43e61a54f8                                                                                                                   │
│ Base      046704f8a556ff360a7f6b8879077e46ca6533e4                                                                                                                   │
│ Target    refs/heads/master                                                                                                                                          │
│ Branches  feature/1                                                                                                                                                  │
│ Commits   ahead 1, behind 1                                                                                                                                          │
│ Status    open                                                                                                                                                       │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ d60e9c1 Add Alan                                                                                                                                                     │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision d2aee03ab1a0b2f54f63a4db54acfa359fc86bb6 with range 046704f8a556ff360a7f6b8879077e46ca6533e4..d60e9c18c50bd5b74df1fadccb340f43e61a54f8 by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

commit d60e9c18c50bd5b74df1fadccb340f43e61a54f8
Author: radicle <radicle@localhost>
Date:   Thu Dec 15 17:28:04 2022 +0000

    Add Alan

diff --git a/CONTRIBUTORS b/CONTRIBUTORS
index 3f60d25..6829c43 100644
--- a/CONTRIBUTORS
+++ b/CONTRIBUTORS
@@ -1 +1,2 @@
 Alice Jones
+Alan K

```

Then, we stack another change onto `feature/1`, adding another contributor:
``` (stderr)
$ git checkout -q -b feature/2 feature/1
$ sed -i '$a Mel Farna' CONTRIBUTORS
$ git commit -a -q -m "Add Mel"
$ git push -o patch.message="Add Mel" rad HEAD:refs/patches
✓ Patch 9450ddf28ead5b44e8aa7d02e52f97809622157c opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

When we look at the patch, we see that it has both commits, because this new
patch uses the same base as the previous patch:
```
$ rad patch show -v 9450ddf28ead5b44e8aa7d02e52f97809622157c
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add Mel                                                                                                                                                    │
│ Patch     9450ddf28ead5b44e8aa7d02e52f97809622157c                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      b4c96fbe0aab7da9762956751b5882fea9bc3e9d                                                                                                                   │
│ Base      046704f8a556ff360a7f6b8879077e46ca6533e4                                                                                                                   │
│ Target    refs/heads/master                                                                                                                                          │
│ Branches  feature/2                                                                                                                                                  │
│ Commits   ahead 2, behind 1                                                                                                                                          │
│ Status    open                                                                                                                                                       │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ b4c96fb Add Mel                                                                                                                                                      │
│ d60e9c1 Add Alan                                                                                                                                                     │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 9450ddf28ead5b44e8aa7d02e52f97809622157c with range 046704f8a556ff360a7f6b8879077e46ca6533e4..b4c96fbe0aab7da9762956751b5882fea9bc3e9d by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

If we want to instead create a "stacked" patch, we can do so with the
`patch.base` push option:

``` (stderr)
$ git push -o patch.message="Add Mel #2" -o patch.base=HEAD^ rad HEAD:refs/patches
✓ Patch 385bfd59a90b66f9fe6e958560b72d5d1a83f7f1 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

As you'll notice, using the previous patch as the base, we only see commit
`b4c96fb` listed for this new patch.

However, since the patch is still intended to be merged into `master`, we see
that it is still two commits ahead and one behind from `master`.

```
$ rad patch show -v 385bfd59a90b66f9fe6e958560b72d5d1a83f7f1
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Title     Add Mel #2                                                                                                                                                 │
│ Patch     385bfd59a90b66f9fe6e958560b72d5d1a83f7f1                                                                                                                   │
│ Author    alice (you)                                                                                                                                                │
│ Head      b4c96fbe0aab7da9762956751b5882fea9bc3e9d                                                                                                                   │
│ Base      d60e9c18c50bd5b74df1fadccb340f43e61a54f8                                                                                                                   │
│ Target    refs/heads/master                                                                                                                                          │
│ Branches  feature/2                                                                                                                                                  │
│ Commits   ahead 2, behind 1                                                                                                                                          │
│ Status    open                                                                                                                                                       │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ b4c96fb Add Mel                                                                                                                                                      │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ● Revision 385bfd59a90b66f9fe6e958560b72d5d1a83f7f1 with range d60e9c18c50bd5b74df1fadccb340f43e61a54f8..b4c96fbe0aab7da9762956751b5882fea9bc3e9d by alice (you) now │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

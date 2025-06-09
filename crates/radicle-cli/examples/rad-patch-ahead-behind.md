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
* 5c88a79 (feature/1) Add Alan
| * e101a99 (HEAD -> master, rad/master) Add Jason
|/ [..]
* f64fb2c Add contributors
* f2de534 Second commit
* 08c788d Initial commit
```

Then we create a patch from `feature/1`:
``` (stderr)
$ git push rad feature/1:refs/patches
✓ Patch 217f050f8891def8fb863f7c0b4f85c89f97299d opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   feature/1 -> refs/patches
```

When listing, we see that it has one addition:
```
$ rad patch list
╭────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title     Author         Reviews  Head     +   -   Updated │
├────────────────────────────────────────────────────────────────────────┤
│ ●  217f050  Add Alan  alice   (you)  -        5c88a79  +1  -0  now     │
╰────────────────────────────────────────────────────────────────────────╯
```

When showing the patch, we see that it is `ahead 1, behind 1`, since master has
diverged by one commit:
```
$ rad patch show -v -p 217f050
╭────────────────────────────────────────────────────╮
│ Title     Add Alan                                 │
│ Patch     217f050f8891def8fb863f7c0b4f85c89f97299d │
│ Author    alice (you)                              │
│ Head      5c88a79d75f5c2b4cc51ee6f163d2db91ee198d7 │
│ Base      f64fb2c8fe28f7c458c72ec8d700373924794943 │
│ Branches  feature/1                                │
│ Commits   ahead 1, behind 1                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 5c88a79 Add Alan                                   │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (5c88a79) now              │
╰────────────────────────────────────────────────────╯

commit 5c88a79d75f5c2b4cc51ee6f163d2db91ee198d7
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
✓ Patch e22ff008e2a0ed47262890d13263031d7555b555 opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

When we look at the patch, we see that it has both commits, because this new
patch uses the same base as the previous patch:
```
$ rad patch show -v e22ff008e2a0ed47262890d13263031d7555b555
╭────────────────────────────────────────────────────╮
│ Title     Add Mel                                  │
│ Patch     e22ff008e2a0ed47262890d13263031d7555b555 │
│ Author    alice (you)                              │
│ Head      7f63fcbcf23fc39eea784c091ad3d20d7e4bd005 │
│ Base      f64fb2c8fe28f7c458c72ec8d700373924794943 │
│ Branches  feature/2                                │
│ Commits   ahead 2, behind 1                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 7f63fcb Add Mel                                    │
│ 5c88a79 Add Alan                                   │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (7f63fcb) now              │
╰────────────────────────────────────────────────────╯
```

If we want to instead create a "stacked" patch, we can do so with the
`patch.base` push option:

``` (stderr)
$ git push -o patch.message="Add Mel #2" -o patch.base=HEAD^ rad HEAD:refs/patches
✓ Patch a467ffa260c4fbe355b6fb550ba0c4956078717e opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

As you'll notice, using the previous patch as the base, we only see commit
`7f63fcb` listed for this new patch.

However, since the patch is still intended to be merged into `master`, we see
that it is still two commits ahead and one behind from `master`.

```
$ rad patch show -v a467ffa260c4fbe355b6fb550ba0c4956078717e
╭────────────────────────────────────────────────────╮
│ Title     Add Mel #2                               │
│ Patch     a467ffa260c4fbe355b6fb550ba0c4956078717e │
│ Author    alice (you)                              │
│ Head      7f63fcbcf23fc39eea784c091ad3d20d7e4bd005 │
│ Base      5c88a79d75f5c2b4cc51ee6f163d2db91ee198d7 │
│ Branches  feature/2                                │
│ Commits   ahead 2, behind 1                        │
│ Status    open                                     │
├────────────────────────────────────────────────────┤
│ 7f63fcb Add Mel                                    │
├────────────────────────────────────────────────────┤
│ ● opened by alice (you) (7f63fcb) now              │
╰────────────────────────────────────────────────────╯
```

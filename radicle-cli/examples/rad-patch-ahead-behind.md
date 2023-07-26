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
✓ Patch 69ebafb6f654fb29d23f630cc165d83d6cbf525c opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   feature/1 -> refs/patches
```

When listing, we see that it has one addition:
```
$ rad patch list
╭─────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title     Author                  Head     +   -   Updated      │
├─────────────────────────────────────────────────────────────────────────────┤
│ ●  69ebafb  Add Alan  z6MknSL…StBU8Vi  (you)  5c88a79  +1  -0  [   ...    ] │
╰─────────────────────────────────────────────────────────────────────────────╯
```

When showing the patch, we see that it is `ahead 1, behind 1`, since master has
diverged by one commit:
```
$ rad patch show -v -p 69ebafb
╭────────────────────────────────────────────────────────────────────╮
│ Title     Add Alan                                                 │
│ Patch     69ebafb6f654fb29d23f630cc165d83d6cbf525c                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      5c88a79d75f5c2b4cc51ee6f163d2db91ee198d7                 │
│ Base      f64fb2c8fe28f7c458c72ec8d700373924794943                 │
│ Branches  feature/1                                                │
│ Commits   ahead 1, behind 1                                        │
│ Status    open                                                     │
├────────────────────────────────────────────────────────────────────┤
│ 5c88a79 Add Alan                                                   │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) [              ...           ]                   │
╰────────────────────────────────────────────────────────────────────╯

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
✓ Patch 53d5f17aba5fd9b7de7a02ecb6f01de561701eeb opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

When we look at the patch, we see that it has both commits, because this new
patch uses the same base as the previous patch:
```
$ rad patch show -v 53d5f17aba5fd9b7de7a02ecb6f01de561701eeb
╭────────────────────────────────────────────────────────────────────╮
│ Title     Add Mel                                                  │
│ Patch     53d5f17aba5fd9b7de7a02ecb6f01de561701eeb                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      7f63fcbcf23fc39eea784c091ad3d20d7e4bd005                 │
│ Base      f64fb2c8fe28f7c458c72ec8d700373924794943                 │
│ Branches  feature/2                                                │
│ Commits   ahead 2, behind 1                                        │
│ Status    open                                                     │
├────────────────────────────────────────────────────────────────────┤
│ 7f63fcb Add Mel                                                    │
│ 5c88a79 Add Alan                                                   │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) [            ...             ]                   │
╰────────────────────────────────────────────────────────────────────╯
```

If we want to instead create a "stacked" patch, we can do so with the
`patch.base` push option:

``` (stderr)
$ git push -o patch.message="Add Mel #2" -o patch.base=5c88a79d75f5c2b4cc51ee6f163d2db91ee198d7 rad HEAD:refs/patches
✓ Patch 459dc67a024ff30c3bca02f0f1e5b746459ce32a opened
To rad://z42hL2jL4XNk6K8oHQaSWfMgCL7ji/z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
 * [new reference]   HEAD -> refs/patches
```

As you'll notice, using the previous patch as the base, we only see commit
`7f63fcb` listed for this new patch.

However, since the patch is still intended to be merged into `master`, we see
that it is still two commits ahead and one behind from `master`.

```
$ rad patch show -v 459dc67a024ff30c3bca02f0f1e5b746459ce32a
╭────────────────────────────────────────────────────────────────────╮
│ Title     Add Mel #2                                               │
│ Patch     459dc67a024ff30c3bca02f0f1e5b746459ce32a                 │
│ Author    did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi │
│ Head      7f63fcbcf23fc39eea784c091ad3d20d7e4bd005                 │
│ Base      5c88a79d75f5c2b4cc51ee6f163d2db91ee198d7                 │
│ Branches  feature/2                                                │
│ Commits   ahead 2, behind 1                                        │
│ Status    open                                                     │
├────────────────────────────────────────────────────────────────────┤
│ 7f63fcb Add Mel                                                    │
├────────────────────────────────────────────────────────────────────┤
│ ● opened by (you) [            ...             ]                   │
╰────────────────────────────────────────────────────────────────────╯
```

When preferred seeds are configured, opening a patch outputs the patch URL.

``` (stderr)
$ git checkout -b changes -q
$ git commit --allow-empty -q -m "Changes"
$ git push rad HEAD:refs/patches
✓ Patch 806276a013152675fe4361e6c15275bd5c8d43b4 opened
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/806276a013152675fe4361e6c15275bd5c8d43b4

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

If we update the patch, the URL is also output.

``` (stderr)
$ git commit --amend --allow-empty -q -m "Other changes"
$ git push -f
✓ Patch 806276a updated to revision 374490d831db6574ddf9e4b8a8a3ef81e6783907
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/806276a013152675fe4361e6c15275bd5c8d43b4

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 + e12525d...b2b6432 changes -> patches/806276a013152675fe4361e6c15275bd5c8d43b4 (forced update)
```

While simply pushing a commit outputs a URL to the new source tree.

``` (stderr)
$ git checkout master -q
$ git merge changes -q
$ git push rad master
✓ Patch 806276a013152675fe4361e6c15275bd5c8d43b4 merged
✓ Canonical head updated to b2b6432af93f8fe188e32d400263021b602cfec8
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/tree/b2b6432af93f8fe188e32d400263021b602cfec8

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   f2de534..b2b6432  master -> master
```

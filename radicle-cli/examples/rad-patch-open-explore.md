When preferred seeds are configured, opening a patch outputs the patch URL.

``` (stderr)
$ git checkout -b changes -q
$ git commit --allow-empty -q -m "Changes"
$ git push rad HEAD:refs/patches
✓ Patch e0b35c56eb265d49cddd72d91cf873f64037d96c opened
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/e0b35c56eb265d49cddd72d91cf873f64037d96c

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

If we update the patch, the URL is also output.

``` (stderr)
$ git commit --amend --allow-empty -q -m "Other changes"
$ git push -f
✓ Patch e0b35c5 updated to revision 0ab4697ba5beee387f1211bdf0880a06564842ce
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/e0b35c56eb265d49cddd72d91cf873f64037d96c

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 + e12525d...b2b6432 changes -> patches/e0b35c56eb265d49cddd72d91cf873f64037d96c (forced update)
```

While simply pushing a commit outputs a URL to the new source tree.

``` (stderr)
$ git checkout master -q
$ git merge changes -q
$ git push rad master
✓ Patch e0b35c56eb265d49cddd72d91cf873f64037d96c merged
✓ Canonical head updated to b2b6432af93f8fe188e32d400263021b602cfec8
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/tree/b2b6432af93f8fe188e32d400263021b602cfec8

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   f2de534..b2b6432  master -> master
```

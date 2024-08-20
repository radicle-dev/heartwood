When preferred seeds are configured, opening a patch outputs the patch URL.

``` (stderr)
$ git checkout -b changes -q
$ git commit --allow-empty -q -m "Changes"
$ git push rad HEAD:refs/patches
✓ Patch c47f80b133e5c0b930cbd21890e4fc535c854c16 opened
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:zPHvWyMMwBBH24oGGtkndq9wZmDC/patches/c47f80b133e5c0b930cbd21890e4fc535c854c16

To rad://zPHvWyMMwBBH24oGGtkndq9wZmDC/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

If we update the patch, the URL is also output.

``` (stderr)
$ git commit --amend --allow-empty -q -m "Other changes"
$ git push -f
✓ Patch c47f80b updated to revision 9c9e16fdeac971460cff8d4dbd4fbfd651bc1e72
To compare against your previous revision c47f80b, run:

   git range-diff f2de534[..] e12525d[..] b2b6432[..]

✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:zPHvWyMMwBBH24oGGtkndq9wZmDC/patches/c47f80b133e5c0b930cbd21890e4fc535c854c16

To rad://zPHvWyMMwBBH24oGGtkndq9wZmDC/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 + e12525d...b2b6432 changes -> patches/c47f80b133e5c0b930cbd21890e4fc535c854c16 (forced update)
```

While simply pushing a commit outputs a URL to the new source tree.

``` (stderr)
$ git checkout master -q
$ git merge changes -q
$ git push rad master
✓ Patch c47f80b133e5c0b930cbd21890e4fc535c854c16 merged
✓ Canonical head for refs/heads/master updated to b2b6432af93f8fe188e32d400263021b602cfec8
✓ Synced with 1 node(s)

  https://app.radicle.xyz/nodes/[..]/rad:zPHvWyMMwBBH24oGGtkndq9wZmDC/tree/b2b6432af93f8fe188e32d400263021b602cfec8

To rad://zPHvWyMMwBBH24oGGtkndq9wZmDC/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   f2de534..b2b6432  master -> master
```

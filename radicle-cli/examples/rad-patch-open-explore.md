When preferred seeds are configured, opening a patch outputs the patch URL.

``` (stderr)
$ git checkout -b changes -q
$ git commit --allow-empty -q -m "Changes"
$ git push rad HEAD:refs/patches
✓ Patch acab0ec777a97d013f30be5d5d1aec32562ecb02 opened
✓ Synced with 1 seed(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/acab0ec777a97d013f30be5d5d1aec32562ecb02

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

If we update the patch, the URL is also output.

``` (stderr)
$ git commit --amend --allow-empty -q -m "Other changes"
$ git push -f
✓ Patch acab0ec updated to revision f7a830d829d0cdf398f63a32b0d5ee31f08e21ab
To compare against your previous revision acab0ec, run:

   git range-diff f2de534[..] e12525d[..] b2b6432[..]

✓ Synced with 1 seed(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/acab0ec777a97d013f30be5d5d1aec32562ecb02

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 + e12525d...b2b6432 changes -> patches/acab0ec777a97d013f30be5d5d1aec32562ecb02 (forced update)
```

While simply pushing a commit outputs a URL to the new source tree.

``` (stderr)
$ git checkout master -q
$ git merge changes -q
$ git push rad master
✓ Patch acab0ec777a97d013f30be5d5d1aec32562ecb02 merged
✓ Canonical head updated to b2b6432af93f8fe188e32d400263021b602cfec8
✓ Synced with 1 seed(s)

  https://app.radicle.xyz/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/tree/b2b6432af93f8fe188e32d400263021b602cfec8

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   f2de534..b2b6432  master -> master
```

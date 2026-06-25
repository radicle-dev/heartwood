When preferred seeds are configured, opening a patch outputs the patch URL.

``` (stderr)
$ git checkout -b changes -q
$ git commit --allow-empty -q -m "Changes"
$ git push rad HEAD:refs/patches
✓ Patch e12aeeb18712c1a37c9f5d596ce30f63102e42c5 opened
✓ Synced with 1 seed(s)

  https://radicle.network/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/e12aeeb18712c1a37c9f5d596ce30f63102e42c5

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 * [new reference]   HEAD -> refs/patches
```

If we update the patch, the URL is also output.

``` (stderr)
$ git commit --amend --allow-empty -q -m "Other changes"
$ git push -f
✓ Patch e12aeeb updated to revision 26b16e0fa7809a5251908d34071ea8b650b7e1c9
To compare against your previous revision e12aeeb, run:

   git range-diff 4c66f0e[..] a9ef335[..] 404b5c7[..]

✓ Synced with 1 seed(s)

  https://radicle.network/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/patches/e12aeeb18712c1a37c9f5d596ce30f63102e42c5

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
 + a9ef335...404b5c7 changes -> patches/e12aeeb18712c1a37c9f5d596ce30f63102e42c5 (forced update)
```

While simply pushing a commit outputs a URL to the new source tree.

``` (stderr)
$ git checkout master -q
$ git merge changes -q
$ git push rad master
✓ Patch e12aeeb18712c1a37c9f5d596ce30f63102e42c5 merged
✓ Canonical reference refs/heads/master updated to target commit 404b5c7a10e51366906fc367994afd44c3b63c68
✓ Synced with 1 seed(s)

  https://radicle.network/nodes/[..]/rad:z3yXbb1sR6UG6ixxV2YF9jUP7ABra/tree/404b5c7a10e51366906fc367994afd44c3b63c68

To rad://z3yXbb1sR6UG6ixxV2YF9jUP7ABra/z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
   4c66f0e..404b5c7  master -> master
```

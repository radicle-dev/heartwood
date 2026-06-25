We may want to checkout a particular revision of a patch.
So first, let's add another change to the patch and a `LICENSE` file.

```
$ touch LICENSE
$ git add LICENSE
$ git commit --message "Add LICENSE, just for the business"
[patch/70a5938 1c229bf] Add LICENSE, just for the business
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
$ git push rad -o patch.message="Add LICENSE, just for the business"
```

We can see the list of revisions of the patch by `show`ing it:

```
$ rad patch show 70a5938
╭──────────────────────────────────────────────────────────╮
│ Title     Define power requirements                      │
│ Patch     70a5938a2620ffad0cef086670112c65f69a3d48       │
│ Author    alice (you)                                    │
│ Head      1c229bff249cb9a82d5058cdb265898c88eb803c       │
│ Base      [..                                    ]       │
│ Target    master                                         │
│ Branches  patch/70a5938                                  │
│ Commits   ahead 3, behind 0                              │
│ Status    open                                           │
│                                                          │
│ See details.                                             │
├──────────────────────────────────────────────────────────┤
│ 1c229bf Add LICENSE, just for the business               │
│ 8083d4c Add README, just for the fun                     │
│ 602ba44 Define power requirements                        │
├──────────────────────────────────────────────────────────┤
│ ● Revision 70a5938 @ [..   ]..602ba44 by alice (you) now │
│ ↑ Revision 052017[..] @ [..   ]..8083d4c by alice (you) now │
│ ↑ Revision 4fc7c3c @ [..   ]..1c229bf by alice (you) now │
╰──────────────────────────────────────────────────────────╯
```

So, let's checkout the previous revision, `0c0942e2`:

```
$ rad patch checkout 70a5938 --revision 052017158836193eb24c54b983614b062cb92bd0 -f
✓ Switched to branch patch/70a5938 at revision 052017[..]
```

And we can confirm that the current commit corresponds to `8083d4c`:

```
$ git rev-parse HEAD
8083d4cbb2b297ade6a12962f8ddc118c3900dcf
```

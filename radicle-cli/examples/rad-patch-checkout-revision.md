We may want to checkout a particular revision of a patch.
So first, let's add another change to the patch and a `LICENSE` file.

```
$ touch LICENSE
$ git add LICENSE
$ git commit --message "Add LICENSE, just for the business"
[patch/c90967c 639f44a] Add LICENSE, just for the business
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
$ git push rad -o patch.message="Add LICENSE, just for the business"
```

We can see the list of revisions of the patch by `show`ing it:

```
$ rad patch show c90967c
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     c90967c43719b916e0b5a8b5dafe353608f8a08a                  │
│ Author    alice (you)                                               │
│ Head      639f44a25145a37f747f3c84265037a9461e44c5                  │
│ Branches  patch/c90967c                                             │
│ Commits   ahead 3, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 639f44a Add LICENSE, just for the business                          │
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by alice (you) (3e674d1) now                               │
│ ↑ updated to 594bb93b4ba836777c111053af7b61ff772afbc5 (27857ec) now │
│ ↑ updated to 92a95e995d436248d844bdd6c94704725efc283d (639f44a) now │
╰─────────────────────────────────────────────────────────────────────╯
```

So, let's checkout the previous revision, `0c0942e2`:

```
$ rad patch checkout c90967c --revision 594bb93b4ba836777c111053af7b61ff772afbc5 -f
✓ Switched to branch patch/c90967c at revision 594bb93
```

And we can confirm that the current commit corresponds to `27857ec`:

```
$ git rev-parse HEAD
27857ec9eb04c69cacab516e8bf4b5fd36090f66
```

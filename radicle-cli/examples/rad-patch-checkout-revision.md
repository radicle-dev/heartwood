We may want to checkout a particular revision of a patch.
So first, let's add another change to the patch and a `LICENSE` file.

```
$ touch LICENSE
$ git add LICENSE
$ git commit --message "Add LICENSE, just for the business"
[patch/aa45913 639f44a] Add LICENSE, just for the business
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
$ git push rad -o patch.message="Add LICENSE, just for the business"
```

We can see the list of revisions of the patch by `show`ing it:

```
$ rad patch show aa45913
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     aa45913e757cacd46972733bddee5472c78fa32a                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      639f44a25145a37f747f3c84265037a9461e44c5                  │
│ Branches  patch/aa45913                                             │
│ Commits   ahead 3, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 639f44a Add LICENSE, just for the business                          │
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) (3e674d1) now                     │
│ ↑ updated to 3156bed9d64d4675d6cf56612d217fc5f4e8a53a (27857ec) now │
│ ↑ updated to 2f5324f61e05cda65b667eeea02570d077a8e724 (639f44a) now │
╰─────────────────────────────────────────────────────────────────────╯
```

So, let's checkout the previous revision, `0c0942e2`:

```
$ rad patch checkout aa45913 --revision 3156bed9d64d4675d6cf56612d217fc5f4e8a53a -f
✓ Switched to branch patch/aa45913
```

And we can confirm that the current commit corresponds to `27857ec`:

```
$ git rev-parse HEAD
27857ec9eb04c69cacab516e8bf4b5fd36090f66
```

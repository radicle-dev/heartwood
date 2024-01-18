We may want to checkout a particular revision of a patch.
So first, let's add another change to the patch and a `LICENSE` file.

```
$ touch LICENSE
$ git add LICENSE
$ git commit --message "Add LICENSE, just for the business"
[patch/0f3cd0b 639f44a] Add LICENSE, just for the business
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
$ git push rad -o patch.message="Add LICENSE, just for the business"
```

We can see the list of revisions of the patch by `show`ing it:

```
$ rad patch show 0f3cd0b
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     0f3cd0b3a69c8f70bfa2d3366122c07704e5bb5f                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      639f44a25145a37f747f3c84265037a9461e44c5                  │
│ Branches  patch/0f3cd0b                                             │
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
│ ↑ updated to 6e6644973e3ecd0965b7bc5743f05a5fe1c7bff9 (27857ec) now │
│ ↑ updated to 9b707980e143c5370d5406050f04d60b705cf849 (639f44a) now │
╰─────────────────────────────────────────────────────────────────────╯
```

So, let's checkout the previous revision, `0c0942e2`:

```
$ rad patch checkout 0f3cd0b --revision 6e66449 -f
✓ Switched to branch patch/0f3cd0b
```

And we can confirm that the current commit corresponds to `27857ec`:

```
$ git rev-parse HEAD
27857ec9eb04c69cacab516e8bf4b5fd36090f66
```

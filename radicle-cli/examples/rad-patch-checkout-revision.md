We may want to checkout a particular revision of a patch.
So first, let's add another change to the patch and a `LICENSE` file.

```
$ touch LICENSE
$ git add LICENSE
$ git commit --message "Add LICENSE, just for the business"
[patch/6ff4f09 639f44a] Add LICENSE, just for the business
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 LICENSE
$ git push rad -o patch.message="Add LICENSE, just for the business"
```

We can see the list of revisions of the patch by `show`ing it:

```
$ rad patch show 6ff4f09
╭─────────────────────────────────────────────────────────────────────╮
│ Title     Define power requirements                                 │
│ Patch     6ff4f09c1b5a81347981f59b02ef43a31a07cdae                  │
│ Author    z6MknSL…StBU8Vi (you)                                     │
│ Head      639f44a25145a37f747f3c84265037a9461e44c5                  │
│ Branches  patch/6ff4f09                                             │
│ Commits   ahead 3, behind 0                                         │
│ Status    open                                                      │
│                                                                     │
│ See details.                                                        │
├─────────────────────────────────────────────────────────────────────┤
│ 639f44a Add LICENSE, just for the business                          │
│ 27857ec Add README, just for the fun                                │
│ 3e674d1 Define power requirements                                   │
├─────────────────────────────────────────────────────────────────────┤
│ ● opened by z6MknSL…StBU8Vi (you) now                               │
│ ↑ updated to 0c0942e2ff2488617d950ede15567ca39a29972e (27857ec) now │
│ ↑ updated to 2bf48de79e371014f084b5501ecc9c9c4182e7fc (639f44a) now │
╰─────────────────────────────────────────────────────────────────────╯
```

So, let's checkout the previous revision, `0c0942e2`:

```
$ rad patch checkout 6ff4f09 --revision 0c0942e2 -f
✓ Switched to branch patch/6ff4f09
```

And we can confirm that the current commit corresponds to `27857ec`:

```
$ git rev-parse HEAD
27857ec9eb04c69cacab516e8bf4b5fd36090f66
```

# CHANGELOG

## 0.28.0

- [`c92d2898`] Update `Oid` parsing to use the new `git2::ObjectFormat` type.

[`c92d2898`]: https://app.radicle.xyz/nodes/seed.radicle.dev/rad:z6cFWeWpnZNHh9rUW8phgA3b5yGt/commits/c92d2898a84b0d35f5a36d6af8c1b51757fd6436

## 0.27.0

### Fixed

- `radicle-surf` has learned how to handle directories that use `[]` in their
  name. This was a limitation in how the `git2` library was used for searching
  "pathspecs".

## 0.25.0

* Update to `radicle-git-ext-0.10.0` [6422fd5](https://app.radicle.xyz/nodes/seed.radicle.xyz/rad:z6cFWeWpnZNHh9rUW8phgA3b5yGt/commits/6422fd580b1c9c96ba40620197e29d7b9fbe2824)

## 0.9.0

This release consists of a major rewrite of this crate. Its API is overall
simplified and is not compatible with the previous version (v0.8.0). The main
changes include:

- `Browser` is removed. Its methods are implemented directly with `Repository`.
- Git will be the only supported VCS. Any extension points for other VCSes were
removed.
- `Ref` and `RefScope` are removed. Reuse the `git-ref-format` crate and a new
`Glob` type for the refspec patterns.
- Added support of `Tree` and `Blob` that correspond to their definitions in
Git.
- Added two new traits `Revision` and `ToCommit` that make methods flexible and
still simple to use.

For more details, please check out the crate's documentation.

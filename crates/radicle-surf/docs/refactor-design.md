# An updated design for radicle-surf

This is a design blueprint for the new `radicle-git/radicle-surf` crate. The
actual design details and implemenation are described and updated in its
documentation comments, viewable via `cargo doc`.

## Introduction

In September 2022, we have ported the [`radicle-surf` crate](https://github.com/radicle-dev/radicle-surf)
from its own github repo to be part of the [`radicle-git` repo](https://github.com/radicle-dev/radicle-git).
We are taking this opportunity to refactor its design as well. Intuitively,
`radicle-surf` provides an API so that one can use it to create a GitHub-like
UI for a git repo:

1. Code browsing: given a specific commit/ref, browse files and directories.
2. Diff between two revisions that resolve into two commits.
3. Retrieve the history of commits with a given head, and optionally a file.
4. List refs and retrieve their metadata: Branches, Tags, Remotes,
Notes and user-defined "categories", where a category is: `refs/<category>/<...>`.

## Motivation

The `radicle-surf` crate aims to provide a safe and easy-to-use API that
supports the features listed in [Introduction](#introduction). Based on the
existing API, the main goals of the refactoring are:

- API review: identify the issues with the current API.
- Updated API: propose an updated API that reuses parts of the existing API.
- Address open issues in the original `radicle-surf` repo.
- Be `git` specific. (i.e. no need to support other VCS systems)
- Remove `git2` from the public API. The use of `git2` should be an
implementation detail.

## API review

In this section, we review some core types in the current API and propose
changes to them. The main theme is to make the API simpler and easier to use.

### Remove the `Browser` type

The type [`Browser`](https://github.com/radicle-dev/radicle-surf/blob/b85d2183d786e5fa447aab9d2f420a32f1061bfa/surf/src/vcs.rs#L145) is awkward:

- it is not a source of truth of any information. For example, `list_branches`
method is just a wrapper of `Repository::list_branches`.
- it takes in `History`, but really operates at the `Snapshot` level.
- it is mutable but its state mutations are not used much.

Can we just remove `Browser` and implement its functionalities using other
types?

- For iteratoring the history, use `History`.
- For generating `Directory`, use `Repository` directly given a `Rev`.
- For accessing `Branch`, `Tag` or `Commit`, use `Repository`.

### Remove the `Snapshot` type

A [`Snapshot`](https://github.com/radicle-dev/radicle-surf/blob/b85d2183d786e5fa447aab9d2f420a32f1061bfa/surf/src/vcs.rs#L140) should be really
just a tree (or `Directory`) of a `Commit` in git. Currently it is a function
that returns a `Directory`. As we are moving to be git specific, we don't need
to have this generic function to create a snapshot across different VCS systems.

The snapshot function can be easily implement as a method of `RepositoryRef`.

### Simplify `Directory` and remove the `Tree` and `Forest` types

The [`Directory`](https://github.com/radicle-dev/radicle-surf/blob/b85d2183d786e5fa447aab9d2f420a32f1061bfa/surf/src/file_system/directory.rs#L144)
type represents the file system view of a snapshot. Its field
`sub_directories` is defined a `Forest` based on `Tree`. The types are
over-engineered from such a simple concept. We could refactor `Directory` to
use `DirectoryContents` for its items and not to use `Tree` or `Forest` at all.

We also found the `list_directory()` method duplicates with `iter()` method.
Hence `list_directory()` is removed, together with `SystemType` type.

### Remove `Vcs` trait

The `Vcs` trait was introduced to support different version control backends,
for example both Git and Pijul, and potentially others. However, since this
port is part of `radicle-git` repo, we are only supporting Git going forward.
We no longer need another layer of indirection defined by `Vcs` trait.

## The new API

With the changes proposed in the previous section, we describe what the new API
would look like and how they meet the requirements.

### Basic types

#### Repository

`Repository` is kept as the entry point of the API, even though its methods
would change due to changes in other types. Also, we would like to consolidate
[`Repository`](https://github.com/radicle-dev/radicle-surf/blob/b85d2183d786e5fa447aab9d2f420a32f1061bfa/surf/src/vcs/git/repo.rs#L53) and
 [`RepositoryRef`](https://github.com/radicle-dev/radicle-surf/blob/b85d2183d786e5fa447aab9d2f420a32f1061bfa/surf/src/vcs/git/repo.rs#L63) to
 simplify the API.

#### Revision and Commit

In Git, `Revision` commonly resolves into a `Commit` but could refer to other
objects for example a `Blob`. Hence we need to keep both concepts in the API.
Currently we have multiple types to identify a `Commit` or `Revision`.

- Commit
- Oid
- Rev

The relations between them are: all `Rev` and `Commit` can resolve into `Oid`,
and in most cases `Rev` can resolve into `Commit`.

On one hand, `Oid` is the ultimate unique identifer but it is more machine-
friendly than human-friendly. On the other hand, `Revision` is most human-
friendly and better suited in the API interface. A conversion from `Revision`
to `Oid` will be useful.

For the places where `Commit` is required, we should explicitly ask for
`Commit` instead of `Revision`.

In conclusion, we define two new traits to support the use of `Revision` and
`Commit`:

```Rust
pub trait Revision {
    /// Resolves a revision into an object id in `repo`.
    fn object_id(&self, repo: &RepositoryRef) -> Result<Oid, Error>;
}

pub trait ToCommit {
    /// Converts to a commit in `repo`.
    fn to_commit(self, repo: &RepositoryRef) -> Result<Commit, Error>;
}
```

These two traits will be implemented for most common representations of
`Revision` and `Commit`, for example `&str`, refs like `Branch`, `Tag`, etc.
Our API will use these traits where we expect a `Revision` or a `Commit`.

#### History

The current `History` is generic over VCS types and also retrieves the full list
of commits when the history is created. The VCS part can be removed and the
history can lazy-load the list of commits by implmenting `Iterator` to support
 potentially very long histories.

We can also store the head commit with the history so that it's easy to get
the start point and it helps to identify the history.

To support getting the history of a file, we provide methods to modify a
`History` to filter by a file path.

The new `History` type would look like this:

```Rust
pub struct History<'a> {
    repo: RepositoryRef<'a>,
    head: Commit,
    revwalk: git2::Revwalk<'a>,
    filter_by: Option<FilterBy>,
}

enum FilterBy {
    File { path: file_system::Path },
}
```

For the methods provided by `History`, please see section [Retrieve the history](#retrieve-the-history) below.

#### Commit

`Commit` is a central concept in Git. In `radicle-surf` we define `Commit` type
to represent its metadata:

```Rust
pub struct Commit {
    /// Object Id
    pub id: Oid,
    /// The author of the commit.
    pub author: Author,
    /// The actor who committed this commit.
    pub committer: Author,
    /// The long form message of the commit.
    pub message: String,
    /// The summary message of the commit.
    pub summary: String,
    /// The parents of this commit.
    pub parents: Vec<Oid>,
}
```

To get the file system snapshot of the commit, the user should use
`root_dir` method described in [Code browsing](#code-browsing) section.

To get the diff of the commit, the user should use `diff_commit` method
described in [Diffs](#diffs) section. Note that we might move that method to
`Commit` type itself.

### Code browsing

The user should be able to browse the files and directories for any given
commit. The core API is:

- Create a root Directory:
```Rust
impl RepositoryRef {
    pub fn root_dir<C: ToCommit>(&self, commit: C) -> Result<Directory, Error>;
}
```

- Browse a Directory's contents:
```Rust
impl Directory {
    pub fn contents(&self) -> impl Iterator<Item = &DirectoryContents>;
}
```
where `DirectoryContents` supports both files and sub-directories:
```Rust
pub enum DirectoryContents {
    /// The `File` variant contains the file's name and the [`File`] itself.
    File {
        /// The name of the file.
        name: Label,
        /// The file data.
        file: File,
    },
    /// The `Directory` variant contains a sub-directory to the current one.
    Directory(Directory),
}
```

### Diffs

The user would be able to create a diff between any two revisions. In the first
implementation, these revisions have to resolve into commits. But in future,
the revisions could refer to other objects, e.g. files (blobs).

The core API is:

```Rust
impl RepositoryRef {
    /// Returns the diff between two revisions.
    pub fn diff<R: Revision>(&self, from: R, to: R) -> Result<Diff, Error>;
}
```

To obtain the diff of a particular commit, we will have:
```Rust
impl RepositoryRef {
    pub fn diff_commit(&self, commit: impl ToCommit) -> Result<Diff, Error>;
}
```

### Retrieve the history

The user would be able to get the list of previous commits reachable from a
particular commit.

To create a `History` from a repo with a given head:
```Rust
impl RepositoryRef {
    pub fn history<C: ToCommit>(&self, head: C) -> Result<History, Error>;
}
```

To access the `History`, the user simply iterates `History` as it implements the
`Iterator` trait that produces `Result<Commit, Error>`.

`History` provides a method to filter based on a path and other helper methods:

```Rust
impl<'a> History<'a> {
    /// Returns the head commit of a history.
    pub fn head(&self) -> &Commit;

    // Modifies a history with a filter by `path`.
    // This is to support getting the history of a file.
    pub fn by_path(mut self, path: file_system::Path) -> Self;
```

- Alternative design:

One potential downside of define `History` as an iterator is that:
`history.next()` takes a mutable history object. A different design is to use
`History` as immutable object that produces an iterator on-demand:

```Rust
pub struct History<'a> {
    repo: RepositoryRef<'a>,
    head: Commit,
}

impl<'a> History<'a> {
    /// This method creats a new `RevWalk` internally and return an
    /// iterator for all commits in a history.
    pub fn iter(&self) -> impl Iterator<Item = Commit>;
}
```

In this design, `History` does not keep `RevWalk` in its state. It will create
a new one when `iter()` is called. I like the immutable interface of this design
but did not implement it in the current code mainly because the libgit2 doc says
[creating a new `RevWalk` is relatively expensive](https://libgit2.org/libgit2/#HEAD/group/revwalk/git_revwalk_new).

### List refs and retrieve their metadata

Git refs are simple names that point to objects using object IDs. In this new
design, we no longer group different refs into a single enum. Instead, each kind
of ref would be their own type, e.g. `Tag`, `Branch`, `Namespace`, etc.

To retrieve the refs, the user would call the `<ref_type>s()` method of a
repo, e.g. `branches()`, `tags()`. The result is an iterator of available refs,
e.g. `Branches`.

```Rust
impl RepositoryRef {
    /// Lists branch names with `filter`.
    pub fn branches<G>(&self, pattern: G) -> Result<Branches, Error>
    where
        G: Into<Glob<Branch>>
}
```

### Git Tree and Blob

In Git, a `Blob` object represents the content of a file, and a `Tree` object
represents the content of a listing (in a directory). A `Tree` or a `Blob` does
not have its own name because there could be different names (paths) pointing
to the same `Tree` or `Blob`. In contrast, a `Directory` or a `File` has names
included and possible other attributes.

In `radicle-surf` API, we would expose `Tree` and `Blob` as defined in Git, i.e.
as the content only, with some extra helper methods, for example `commit()` that
returns the commit that created the object.

The `Repository` provides methods to retrieve `Tree` or `Blob` for a given path.

## Summary

This design document was created as a guideline for refactoring the
`radicle-surf` crate when we move it into the `radicle-git` repo. As the code
evolves, this document would not be matching the actual code exactly. However
we can still come back to this document to learn about the history of the
design and update it when needed.

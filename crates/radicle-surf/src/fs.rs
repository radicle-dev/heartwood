//! Definition for a file system consisting of `Directory` and `File`.
//!
//! A `Directory` is expected to be a non-empty tree of directories and files.
//! See [`Directory`] for more information.

use std::{
    cmp::Ordering,
    collections::BTreeMap,
    convert::{Infallible, Into as _},
    path::{Path, PathBuf},
};

use git2::Blob;
use radicle_oid::Oid;
use url::Url;

use crate::{Repository, Revision};

pub mod error {
    use std::path::PathBuf;

    use thiserror::Error;

    #[derive(Debug, Error, PartialEq)]
    pub enum Directory {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        File(#[from] File),
        #[error("the path {0} is not valid")]
        InvalidPath(PathBuf),
        #[error("the entry at '{0}' must be of type {1}")]
        InvalidType(PathBuf, &'static str),
        #[error("the entry name was not valid UTF-8")]
        Utf8Error,
        #[error("the path {0} not found")]
        PathNotFound(PathBuf),
        #[error(transparent)]
        Submodule(#[from] Submodule),
    }

    #[derive(Debug, Error, PartialEq)]
    pub enum File {
        #[error(transparent)]
        Git(#[from] git2::Error),
    }

    #[derive(Debug, Error, PartialEq)]
    pub enum Submodule {
        #[error("URL is invalid utf-8 for submodule '{name}': {err}")]
        Utf8 {
            name: String,
            #[source]
            err: std::str::Utf8Error,
        },
        #[error("failed to parse URL '{url}' for submodule '{name}': {err}")]
        ParseUrl {
            name: String,
            url: String,
            #[source]
            err: url::ParseError,
        },
    }
}

/// A `File` in a git repository.
///
/// The representation is lightweight and contains the [`Oid`] that
/// points to the git blob which is this file.
///
/// The name of a file can be retrieved via [`File::name`].
///
/// The [`FileContent`] of a file can be retrieved via
/// [`File::content`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct File {
    /// The name of the file.
    name: String,
    /// The relative path of the file, not including the `name`,
    /// in respect to the root of the git repository.
    prefix: PathBuf,
    /// The object identifier of the git blob of this file.
    id: Oid,
}

impl File {
    /// Construct a new `File`.
    ///
    /// The `path` must be the prefix location of the directory, and
    /// so should not end in `name`.
    ///
    /// The `id` must point to a git blob.
    pub(crate) fn new(name: String, prefix: PathBuf, id: Oid) -> Self {
        debug_assert!(
            !prefix.ends_with(&name),
            "prefix = {prefix:?}, name = {name}",
        );
        Self { name, prefix, id }
    }

    /// The name of this `File`.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// The object identifier of this `File`.
    pub fn id(&self) -> Oid {
        self.id
    }

    /// Return the exact path for this `File`, including the `name` of
    /// the directory itself.
    ///
    /// The path is relative to the git repository root.
    pub fn path(&self) -> PathBuf {
        self.prefix.join(&self.name)
    }

    /// Return the [`Path`] where this `File` is located, relative to the
    /// git repository root.
    pub fn location(&self) -> &Path {
        &self.prefix
    }

    /// Get the [`FileContent`] for this `File`.
    ///
    /// # Errors
    ///
    /// This function will fail if it could not find the `git` blob
    /// for the `Oid` of this `File`.
    pub fn content<'a>(&self, repo: &'a Repository) -> Result<FileContent<'a>, error::File> {
        let blob = repo.find_blob(self.id)?;
        Ok(FileContent { blob })
    }
}

/// The contents of a [`File`].
///
/// To construct a `FileContent` use [`File::content`].
pub struct FileContent<'a> {
    blob: Blob<'a>,
}

impl<'a> FileContent<'a> {
    /// Return the file contents as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        self.blob.content()
    }

    /// Return the size of the file contents.
    pub fn size(&self) -> usize {
        self.blob.size()
    }

    /// Creates a `FileContent` using a blob.
    pub(crate) fn new(blob: Blob<'a>) -> Self {
        Self { blob }
    }
}

/// A representations of a [`Directory`]'s entries.
pub struct Entries {
    listing: BTreeMap<String, Entry>,
}

impl Entries {
    /// Return the name of each [`Entry`].
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.listing.keys()
    }

    /// Return each [`Entry`].
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.listing.values()
    }

    /// Return each [`Entry`] and its name.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Entry)> {
        self.listing.iter()
    }
}

impl Iterator for Entries {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        // Can be improved when `pop_first()` is stable for BTreeMap.
        let next_key = {
            let k = self.listing.keys().next()?;
            k.clone()
        };
        self.listing.remove(&next_key)
    }
}

/// An `Entry` is either a [`File`] entry or a [`Directory`] entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A file entry within a [`Directory`].
    File(File),
    /// A sub-directory of a [`Directory`].
    Directory(Directory),
    /// An entry points to a submodule.
    Submodule(Submodule),
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Entry::File(x), Entry::File(y)) => x.name().cmp(y.name()),
            (Entry::File(_), Entry::Directory(_)) => Ordering::Less,
            (Entry::File(_), Entry::Submodule(_)) => Ordering::Less,
            (Entry::Directory(_), Entry::File(_)) => Ordering::Greater,
            (Entry::Submodule(_), Entry::File(_)) => Ordering::Less,
            (Entry::Directory(x), Entry::Directory(y)) => x.name().cmp(y.name()),
            (Entry::Directory(x), Entry::Submodule(y)) => x.name().cmp(y.name()),
            (Entry::Submodule(x), Entry::Directory(y)) => x.name().cmp(y.name()),
            (Entry::Submodule(x), Entry::Submodule(y)) => x.name().cmp(y.name()),
        }
    }
}

impl Entry {
    /// Get a label for the `Entries`, either the name of the [`File`],
    /// the name of the [`Directory`], or the name of the [`Submodule`].
    pub fn name(&self) -> &String {
        match self {
            Entry::File(file) => &file.name,
            Entry::Directory(directory) => directory.name(),
            Entry::Submodule(submodule) => submodule.name(),
        }
    }

    pub fn path(&self) -> PathBuf {
        match self {
            Entry::File(file) => file.path(),
            Entry::Directory(directory) => directory.path(),
            Entry::Submodule(submodule) => submodule.path(),
        }
    }

    pub fn location(&self) -> &Path {
        match self {
            Entry::File(file) => file.location(),
            Entry::Directory(directory) => directory.location(),
            Entry::Submodule(submodule) => submodule.location(),
        }
    }

    /// Returns `true` if the `Entry` is a file.
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File(_))
    }

    /// Returns `true` if the `Entry` is a directory.
    pub fn is_directory(&self) -> bool {
        matches!(self, Entry::Directory(_))
    }

    pub(crate) fn from_entry(
        entry: &git2::TreeEntry,
        path: PathBuf,
        repo: &Repository,
    ) -> Result<Self, error::Directory> {
        let name = entry
            .name()
            .map_err(|_| error::Directory::Utf8Error)?
            .to_string();
        let id = entry.id().into();

        match entry.kind() {
            Some(git2::ObjectType::Tree) => Ok(Self::Directory(Directory::new(name, path, id))),
            Some(git2::ObjectType::Blob) => Ok(Self::File(File::new(name, path, id))),
            Some(git2::ObjectType::Commit) => {
                let submodule = (!repo.is_bare())
                    .then(|| repo.find_submodule(&name))
                    .transpose()?;
                Ok(Self::Submodule(Submodule::new(name, path, submodule, id)?))
            }
            _ => Err(error::Directory::InvalidType(path, "tree or blob")),
        }
    }
}

/// A `Directory` is the representation of a file system directory, for a given
/// [`git` tree][git-tree].
///
/// The name of a directory can be retrieved via [`File::name`].
///
/// The [`Entries`] of a directory can be retrieved via
/// [`Directory::entries`].
///
/// [git-tree]: https://git-scm.com/book/en/v2/Git-Internals-Git-Objects
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    /// The name of the directory.
    name: String,
    /// The relative path of the directory, not including the `name`,
    /// in respect to the root of the git repository.
    prefix: PathBuf,
    /// The object identifier of the git tree of this directory.
    id: Oid,
}

const ROOT_DIR: &str = "";

impl Directory {
    /// Creates a directory given its `tree_id`.
    ///
    /// The `name` and `prefix` are both set to be empty.
    pub(crate) fn root(id: Oid) -> Self {
        Self::new(ROOT_DIR.to_string(), PathBuf::new(), id)
    }

    /// Creates a directory given its `name` and `id`.
    ///
    /// The `path` must be the prefix location of the directory, and
    /// so should not end in `name`.
    ///
    /// The `id` must point to a `git` tree.
    pub(crate) fn new(name: String, prefix: PathBuf, id: Oid) -> Self {
        debug_assert!(
            name.is_empty() || !prefix.ends_with(&name),
            "prefix = {prefix:?}, name = {name}",
        );
        Self { name, prefix, id }
    }

    /// Get the name of the current `Directory`.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// The object identifier of this `[Directory]`.
    pub fn id(&self) -> Oid {
        self.id
    }

    /// Return the exact path for this `Directory`, including the `name` of the
    /// directory itself.
    ///
    /// The path is relative to the git repository root.
    pub fn path(&self) -> PathBuf {
        self.prefix.join(&self.name)
    }

    /// Return the [`Path`] where this `Directory` is located, relative to the
    /// git repository root.
    pub fn location(&self) -> &Path {
        &self.prefix
    }

    /// Return the [`Entries`] for this `Directory`'s `Oid`.
    ///
    /// The resulting `Entries` will only resolve to this
    /// `Directory`'s entries. Any sub-directories will need to be
    /// resolved independently.
    ///
    /// # Errors
    ///
    /// This function will fail if it could not find the `git` tree
    /// for the `Oid`.
    pub fn entries(&self, repo: &Repository) -> Result<Entries, error::Directory> {
        let tree = repo.find_tree(self.id)?;

        let mut entries = BTreeMap::new();
        let mut error = None;
        let path = self.path();

        // Walks only the first level of entries. And `_entry_path` is always
        // empty for the first level.
        tree.walk(git2::TreeWalkMode::PreOrder, |_entry_path, entry| {
            match Entry::from_entry(entry, path.clone(), repo) {
                Ok(entry) => match entry {
                    Entry::File(_) => {
                        entries.insert(entry.name().clone(), entry);
                        git2::TreeWalkResult::Ok
                    }
                    Entry::Directory(_) => {
                        entries.insert(entry.name().clone(), entry);
                        // Skip nested directories
                        git2::TreeWalkResult::Skip
                    }
                    Entry::Submodule(_) => {
                        entries.insert(entry.name().clone(), entry);
                        git2::TreeWalkResult::Ok
                    }
                },
                Err(err) => {
                    error = Some(err);
                    git2::TreeWalkResult::Abort
                }
            }
        })?;

        match error {
            Some(err) => Err(err),
            None => Ok(Entries { listing: entries }),
        }
    }

    /// Find the [`Entry`] found at a non-empty `path`, if it exists.
    pub fn find_entry<P>(&self, path: &P, repo: &Repository) -> Result<Entry, error::Directory>
    where
        P: AsRef<Path>,
    {
        // Search the path in git2 tree.
        let path = path.as_ref();
        let git2_tree = repo.find_tree(self.id)?;
        let entry = git2_tree.get_path(path).map_err(|err| {
            if err.code() == git2::ErrorCode::NotFound {
                error::Directory::PathNotFound(path.to_path_buf())
            } else {
                err.into()
            }
        })?;
        let parent = path
            .parent()
            .ok_or_else(|| error::Directory::InvalidPath(path.to_path_buf()))?;
        let root_path = self.path().join(parent);

        Entry::from_entry(&entry, root_path, repo)
    }

    /// Find the `Oid`, for a [`File`], found at `path`, if it exists.
    pub fn find_file<P>(&self, path: &P, repo: &Repository) -> Result<File, error::Directory>
    where
        P: AsRef<Path>,
    {
        match self.find_entry(path, repo)? {
            Entry::File(file) => Ok(file),
            _ => Err(error::Directory::InvalidType(
                path.as_ref().to_path_buf(),
                "file",
            )),
        }
    }

    /// Find the `Directory` found at `path`, if it exists.
    ///
    /// If `path` is `ROOT_DIR` (i.e. an empty path), returns self.
    pub fn find_directory<P>(&self, path: &P, repo: &Repository) -> Result<Self, error::Directory>
    where
        P: AsRef<Path>,
    {
        if path.as_ref() == Path::new(ROOT_DIR) {
            return Ok(self.clone());
        }

        match self.find_entry(path, repo)? {
            Entry::Directory(d) => Ok(d),
            _ => Err(error::Directory::InvalidType(
                path.as_ref().to_path_buf(),
                "directory",
            )),
        }
    }

    // TODO(fintan): This is going to be a bit trickier so going to leave it out for
    // now
    #[allow(dead_code)]
    fn fuzzy_find(_label: &Path) -> Vec<Self> {
        unimplemented!()
    }

    /// Get the total size, in bytes, of a `Directory`. The size is
    /// the sum of all files that can be reached from this `Directory`.
    pub fn size(&self, repo: &Repository) -> Result<usize, error::Directory> {
        self.traverse(repo, 0, &mut |size, entry| match entry {
            Entry::File(file) => Ok(size + file.content(repo)?.size()),
            Entry::Directory(dir) => Ok(size + dir.size(repo)?),
            Entry::Submodule(_) => Ok(size),
        })
    }

    /// Traverse the entire `Directory` using the `initial`
    /// accumulator and the function `f`.
    ///
    /// For each [`Entry::Directory`] this will recursively call
    /// [`Directory::traverse`] and obtain its [`Entries`].
    ///
    /// `Error` is the error type of the fallible function.
    /// `B` is the type of the accumulator.
    /// `F` is the fallible function that takes the accumulator and
    /// the next [`Entry`], possibly providing the next accumulator
    /// value.
    pub fn traverse<Error, B, F>(
        &self,
        repo: &Repository,
        initial: B,
        f: &mut F,
    ) -> Result<B, Error>
    where
        Error: From<error::Directory>,
        F: FnMut(B, &Entry) -> Result<B, Error>,
    {
        self.entries(repo)?
            .entries()
            .try_fold(initial, |acc, entry| match entry {
                Entry::File(_) => f(acc, entry),
                Entry::Directory(directory) => {
                    let acc = directory.traverse(repo, acc, f)?;
                    f(acc, entry)
                }
                Entry::Submodule(_) => f(acc, entry),
            })
    }
}

impl Revision for Directory {
    type Error = Infallible;

    fn object_id(&self, _repo: &Repository) -> Result<Oid, Self::Error> {
        Ok(self.id)
    }
}

/// A representation of a Git [submodule] when encountered in a Git
/// repository.
///
/// [submodule]: https://git-scm.com/book/en/v2/Git-Tools-Submodules
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    name: String,
    prefix: PathBuf,
    id: Oid,
    url: Option<Url>,
}

impl Submodule {
    /// Construct a new `Submodule`.
    ///
    /// The `path` must be the prefix location of the directory, and
    /// so should not end in `name`.
    ///
    /// The `id` is the commit pointer that Git provides when listing
    /// a submodule.
    pub fn new(
        name: String,
        prefix: PathBuf,
        submodule: Option<git2::Submodule>,
        id: Oid,
    ) -> Result<Self, error::Submodule> {
        let url = submodule
            .and_then(|module| {
                module
                    .opt_url_bytes()
                    .map(|bs| std::str::from_utf8(bs).map(|url| url.to_string()))
            })
            .transpose()
            .map_err(|err| error::Submodule::Utf8 {
                name: name.clone(),
                err,
            })?;
        let url = url
            .map(|url| {
                Url::parse(&url).map_err(|err| error::Submodule::ParseUrl {
                    name: name.clone(),
                    url,
                    err,
                })
            })
            .transpose()?;
        Ok(Self {
            name,
            prefix,
            id,
            url,
        })
    }

    /// The name of this `Submodule`.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Return the [`Path`] where this `Submodule` is located, relative to the
    /// git repository root.
    pub fn location(&self) -> &Path {
        &self.prefix
    }

    /// Return the exact path for this `Submodule`, including the
    /// `name` of the submodule itself.
    ///
    /// The path is relative to the git repository root.
    pub fn path(&self) -> PathBuf {
        self.prefix.join(&self.name)
    }

    /// The object identifier of this `Submodule`.
    ///
    /// Note that this does not exist in the parent `Repository`. A
    /// new `Repository` should be opened for the submodule.
    pub fn id(&self) -> Oid {
        self.id
    }

    /// The URL for the submodule, if it is defined.
    pub fn url(&self) -> &Option<Url> {
        &self.url
    }
}

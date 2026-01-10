use std::{
    collections::BTreeSet,
    convert::TryFrom,
    path::{Path, PathBuf},
    str,
};

use radicle_git_ref_format::{Qualified, RefStr, RefString, refspec::QualifiedPattern};
use radicle_oid::Oid;

use crate::{
    Branch, Commit, Error, Glob, History, Namespace, Revision, Signature, Stats, Tag, ToCommit,
    blob::{Blob, BlobRef},
    diff::{Diff, FileDiff},
    fs::{Directory, File, FileContent},
    refs::{BranchNames, Branches, Categories, Namespaces, TagNames, Tags},
    tree::{Entry, Tree},
};

/// Enumeration of errors that can occur in repo operations.
pub mod error {
    use std::path::PathBuf;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Repo {
        #[error("path not found for: {0}")]
        PathNotFound(PathBuf),
    }
}

/// Represents the state associated with a Git repository.
///
/// Many other types in this crate are derived from methods in this struct.
pub struct Repository {
    /// Wrapper around the `git2`'s `git2::Repository` type.
    /// This is to to limit the functionality that we can do
    /// on the underlying object.
    inner: git2::Repository,
}

////////////////////////////////////////////
// Public API, ONLY add `pub fn` in here. //
////////////////////////////////////////////
impl Repository {
    /// Open a git repository given its exact URI.
    ///
    /// # Errors
    ///
    /// * [`Error::Git`]
    pub fn open(repo_uri: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let repo = git2::Repository::open(repo_uri)?;
        Ok(Self { inner: repo })
    }

    /// Attempt to open a git repository at or above `repo_uri` in the file
    /// system.
    pub fn discover(repo_uri: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let repo = git2::Repository::discover(repo_uri)?;
        Ok(Self { inner: repo })
    }

    /// What is the current namespace we're browsing in.
    pub fn which_namespace(&self) -> Result<Option<Namespace>, Error> {
        self.inner
            .namespace_bytes()
            .map(|ns| Namespace::try_from(ns).map_err(Error::from))
            .transpose()
    }

    /// Switch to a `namespace`
    pub fn switch_namespace(&self, namespace: &RefString) -> Result<(), Error> {
        Ok(self.inner.set_namespace(namespace.as_str())?)
    }

    pub fn with_namespace<T, F>(&self, namespace: &RefString, f: F) -> Result<T, Error>
    where
        F: FnOnce() -> Result<T, Error>,
    {
        self.switch_namespace(namespace)?;
        let res = f();
        self.inner.remove_namespace()?;
        res
    }

    /// Returns an iterator of branches that match `pattern`.
    pub fn branches<'a, G>(&'a self, pattern: G) -> Result<Branches<'a>, Error>
    where
        G: Into<Glob<Branch>>,
    {
        let pattern = pattern.into();
        let mut branches = Branches::default();
        for glob in pattern.globs() {
            let namespaced = self.namespaced_pattern(glob)?;
            let references = self.inner.references_glob(&namespaced)?;
            branches.push(references);
        }
        Ok(branches)
    }

    /// Lists branch names with `filter`.
    pub fn branch_names<'a, G>(&'a self, filter: G) -> Result<BranchNames<'a>, Error>
    where
        G: Into<Glob<Branch>>,
    {
        Ok(self.branches(filter)?.names())
    }

    /// Returns an iterator of tags that match `pattern`.
    pub fn tags<'a>(&'a self, pattern: &Glob<Tag>) -> Result<Tags<'a>, Error> {
        let mut tags = Tags::default();
        for glob in pattern.globs() {
            let namespaced = self.namespaced_pattern(glob)?;
            let references = self.inner.references_glob(&namespaced)?;
            tags.push(references);
        }
        Ok(tags)
    }

    /// Lists tag names in the local RefScope.
    pub fn tag_names<'a>(&'a self, filter: &Glob<Tag>) -> Result<TagNames<'a>, Error> {
        Ok(self.tags(filter)?.names())
    }

    pub fn categories<'a>(
        &'a self,
        pattern: &Glob<Qualified<'_>>,
    ) -> Result<Categories<'a>, Error> {
        let mut cats = Categories::default();
        for glob in pattern.globs() {
            let namespaced = self.namespaced_pattern(glob)?;
            let references = self.inner.references_glob(&namespaced)?;
            cats.push(references);
        }
        Ok(cats)
    }

    /// Returns an iterator of namespaces that match `pattern`.
    pub fn namespaces(&self, pattern: &Glob<Namespace>) -> Result<Namespaces, Error> {
        let mut set = BTreeSet::new();
        for glob in pattern.globs() {
            let new_set = self
                .inner
                .references_glob(glob)?
                .map(|reference| {
                    reference
                        .map_err(Error::Git)
                        .and_then(|r| Namespace::try_from(&r).map_err(Error::from))
                })
                .collect::<Result<BTreeSet<Namespace>, Error>>()?;
            set.extend(new_set);
        }
        Ok(Namespaces::new(set))
    }

    /// Get the [`Diff`] between two commits.
    pub fn diff(&self, from: impl Revision, to: impl Revision) -> Result<Diff, Error> {
        let from_commit = self.find_commit(self.object_id(&from)?)?;
        let to_commit = self.find_commit(self.object_id(&to)?)?;
        self.diff_commits(None, Some(&from_commit), &to_commit)
            .and_then(|diff| Diff::try_from(diff).map_err(Error::from))
    }

    /// Get the [`Diff`] of a `commit`.
    ///
    /// If the `commit` has a parent, then it the diff will be a
    /// comparison between itself and that parent. Otherwise, the left
    /// hand side of the diff will pass nothing.
    pub fn diff_commit(&self, commit: impl ToCommit) -> Result<Diff, Error> {
        let commit = commit
            .to_commit(self)
            .map_err(|err| Error::ToCommit(err.into()))?;
        match commit.parents.first() {
            Some(parent) => self.diff(*parent, commit.id),
            None => self.initial_diff(commit.id),
        }
    }

    /// Get the [`FileDiff`] between two revisions for a file at `path`.
    ///
    /// If `path` is only a directory name, not a file, returns
    /// a [`FileDiff`] for any file under `path`.
    pub fn diff_file<P: AsRef<Path>, R: Revision>(
        &self,
        path: &P,
        from: R,
        to: R,
    ) -> Result<FileDiff, Error> {
        let from_commit = self.find_commit(self.object_id(&from)?)?;
        let to_commit = self.find_commit(self.object_id(&to)?)?;
        let diff = self
            .diff_commits(Some(path.as_ref()), Some(&from_commit), &to_commit)
            .and_then(|diff| Diff::try_from(diff).map_err(Error::from))?;
        let file_diff = diff
            .into_files()
            .pop()
            .ok_or(error::Repo::PathNotFound(path.as_ref().to_path_buf()))?;
        Ok(file_diff)
    }

    /// Parse an [`Oid`] from the given string.
    pub fn oid(&self, oid: &str) -> Result<Oid, Error> {
        Ok(self.inner.revparse_single(oid)?.id().into())
    }

    /// Returns a top level `Directory` without nested sub-directories.
    ///
    /// To visit inside any nested sub-directories, call `directory.get(&repo)`
    /// on the sub-directory.
    pub fn root_dir<C: ToCommit>(&self, commit: C) -> Result<Directory, Error> {
        let commit = commit
            .to_commit(self)
            .map_err(|err| Error::ToCommit(err.into()))?;
        let git2_commit = self.inner.find_commit((commit.id).into())?;
        let tree = git2_commit.as_object().peel_to_tree()?;
        Ok(Directory::root(tree.id().into()))
    }

    /// Returns a [`Directory`] for `path` in `commit`.
    pub fn directory<C: ToCommit, P: AsRef<Path>>(
        &self,
        commit: C,
        path: &P,
    ) -> Result<Directory, Error> {
        let root = self.root_dir(commit)?;
        Ok(root.find_directory(path, self)?)
    }

    /// Returns a [`File`] for `path` in `commit`.
    pub fn file<C: ToCommit, P: AsRef<Path>>(&self, commit: C, path: &P) -> Result<File, Error> {
        let root = self.root_dir(commit)?;
        Ok(root.find_file(path, self)?)
    }

    /// Returns a [`Tree`] for `path` in `commit`.
    pub fn tree<C: ToCommit, P: AsRef<Path>>(&self, commit: C, path: &P) -> Result<Tree, Error> {
        let commit = commit
            .to_commit(self)
            .map_err(|e| Error::ToCommit(e.into()))?;
        let dir = self.directory(commit.id, path)?;
        let mut entries = dir
            .entries(self)?
            .map(|en| {
                let name = en.name().to_string();
                let path = en.path();
                Ok(Entry::new(name, path, en.into(), commit.clone()))
            })
            .collect::<Result<Vec<Entry>, Error>>()?;
        entries.sort();

        Ok(Tree::new(
            dir.id(),
            entries,
            commit,
            path.as_ref().to_path_buf(),
        ))
    }

    /// Returns a [`Blob`] for `path` in `commit`.
    pub fn blob<'a, C: ToCommit, P: AsRef<Path>>(
        &'a self,
        commit: C,
        path: &P,
    ) -> Result<Blob<BlobRef<'a>>, Error> {
        let commit = commit
            .to_commit(self)
            .map_err(|e| Error::ToCommit(e.into()))?;
        let file = self.file(commit.id, path)?;
        let last_commit = self
            .last_commit(path, commit)?
            .ok_or_else(|| error::Repo::PathNotFound(path.as_ref().to_path_buf()))?;
        let git2_blob = self.find_blob(file.id())?;
        Ok(Blob::<BlobRef<'a>>::new(file.id(), git2_blob, last_commit))
    }

    pub fn blob_ref(&self, oid: Oid) -> Result<BlobRef<'_>, Error> {
        Ok(BlobRef {
            inner: self.find_blob(oid)?,
        })
    }

    /// Returns the last commit, if exists, for a `path` in the history of
    /// `rev`.
    pub fn last_commit<P, C>(&self, path: &P, rev: C) -> Result<Option<Commit>, Error>
    where
        P: AsRef<Path>,
        C: ToCommit,
    {
        let history = self.history(rev)?;
        history.by_path(path).next().transpose()
    }

    /// Returns a commit for `rev`, if it exists.
    pub fn commit<R: Revision>(&self, rev: R) -> Result<Commit, Error> {
        rev.to_commit(self)
    }

    /// Gets the [`Stats`] of this repository starting from the
    /// `HEAD` (see [`Repository::head`]) of the repository.
    pub fn stats(&self) -> Result<Stats, Error> {
        self.stats_from(&self.head()?)
    }

    /// Gets the [`Stats`] of this repository starting from the given
    /// `rev`.
    pub fn stats_from<R>(&self, rev: &R) -> Result<Stats, Error>
    where
        R: Revision,
    {
        let branches = self.branches(Glob::all_heads())?.count();
        let mut history = self.history(rev)?;
        let (commits, contributors) = history.try_fold(
            (0, BTreeSet::new()),
            |(commits, mut contributors), commit| {
                let commit = commit?;
                contributors.insert((commit.author.name, commit.author.email));
                Ok::<_, Error>((commits + 1, contributors))
            },
        )?;
        Ok(Stats {
            branches,
            commits,
            contributors: contributors.len(),
        })
    }

    // TODO(finto): I think this can be removed in favour of using
    // `source::Blob::new`
    /// Retrieves the file with `path` in this commit.
    pub fn get_commit_file<'a, P, R>(&'a self, rev: &R, path: &P) -> Result<FileContent<'a>, Error>
    where
        P: AsRef<Path>,
        R: Revision,
    {
        let path = path.as_ref();
        let id = self.object_id(rev)?;
        let commit = self.find_commit(id)?;
        let tree = commit.tree()?;
        let entry = tree.get_path(path)?;
        let object = entry.to_object(&self.inner)?;
        let blob = object
            .into_blob()
            .map_err(|_| error::Repo::PathNotFound(path.to_path_buf()))?;
        Ok(FileContent::new(blob))
    }

    /// Returns the [`Oid`] of the current `HEAD`.
    pub fn head(&self) -> Result<Oid, Error> {
        let head = self.inner.head()?;
        let head_commit = head.peel_to_commit()?;
        Ok(head_commit.id().into())
    }

    /// Extract the signature from a commit
    ///
    /// # Arguments
    ///
    /// `field` - the name of the header field containing the signature block;
    ///           pass `None` to extract the default 'gpgsig'
    pub fn extract_signature(
        &self,
        commit: impl ToCommit,
        field: Option<&str>,
    ) -> Result<Option<Signature>, Error> {
        // Match is necessary here because according to the documentation for
        // git_commit_extract_signature at
        // https://libgit2.org/libgit2/#HEAD/group/commit/git_commit_extract_signature
        // the return value for a commit without a signature will be GIT_ENOTFOUND
        let commit = commit
            .to_commit(self)
            .map_err(|e| Error::ToCommit(e.into()))?;

        match self.inner.extract_signature(&commit.id.into(), field) {
            Err(error) => {
                if error.code() == git2::ErrorCode::NotFound {
                    Ok(None)
                } else {
                    Err(error.into())
                }
            }
            Ok(sig) => Ok(Some(Signature::from(sig.0))),
        }
    }

    /// Returns the history with the `head` commit.
    pub fn history<'a, C: ToCommit>(&'a self, head: C) -> Result<History<'a>, Error> {
        History::new(self, head)
    }

    /// Lists branches that are reachable from `rev`.
    pub fn revision_branches(
        &self,
        rev: impl Revision,
        glob: Glob<Branch>,
    ) -> Result<Vec<Branch>, Error> {
        let oid = self.object_id(&rev)?;
        let mut contained_branches = vec![];
        for branch in self.branches(glob)? {
            let branch = branch?;
            let namespaced = self.namespaced_refname(&branch.refname())?;
            let reference = self.inner.find_reference(namespaced.as_str())?;
            if self.reachable_from(&reference, &oid)? {
                contained_branches.push(branch);
            }
        }

        Ok(contained_branches)
    }

    pub fn object_format(&self) -> git2::ObjectFormat {
        self.inner.object_format()
    }
}

////////////////////////////////////////////////////////////
// Private API, ONLY add `pub(crate) fn` or `fn` in here. //
////////////////////////////////////////////////////////////
impl Repository {
    pub(crate) fn is_bare(&self) -> bool {
        self.inner.is_bare()
    }

    pub(crate) fn find_submodule<'a>(
        &'a self,
        name: &str,
    ) -> Result<git2::Submodule<'a>, git2::Error> {
        self.inner.find_submodule(name)
    }

    pub(crate) fn find_blob(&self, oid: Oid) -> Result<git2::Blob<'_>, git2::Error> {
        self.inner.find_blob(oid.into())
    }

    pub(crate) fn find_commit(&self, oid: Oid) -> Result<git2::Commit<'_>, git2::Error> {
        self.inner.find_commit(oid.into())
    }

    pub(crate) fn find_tree(&self, oid: Oid) -> Result<git2::Tree<'_>, git2::Error> {
        self.inner.find_tree(oid.into())
    }

    pub(crate) fn refname_to_id<R>(&self, name: &R) -> Result<Oid, git2::Error>
    where
        R: AsRef<RefStr>,
    {
        self.inner
            .refname_to_id(name.as_ref().as_str())
            .map(Oid::from)
    }

    pub(crate) fn revwalk(&self) -> Result<git2::Revwalk<'_>, git2::Error> {
        self.inner.revwalk()
    }

    pub(super) fn object_id<R: Revision>(&self, r: &R) -> Result<Oid, Error> {
        r.object_id(self).map_err(|err| Error::Revision(err.into()))
    }

    /// Get the [`Diff`] of a commit with no parents.
    fn initial_diff<R: Revision>(&self, rev: R) -> Result<Diff, Error> {
        let commit = self.find_commit(self.object_id(&rev)?)?;
        self.diff_commits(None, None, &commit)
            .and_then(|diff| Diff::try_from(diff).map_err(Error::from))
    }

    fn reachable_from(&self, reference: &git2::Reference, oid: &Oid) -> Result<bool, Error> {
        let git2_oid = (*oid).into();
        let other = reference.peel_to_commit()?.id();
        let is_descendant = self.inner.graph_descendant_of(other, git2_oid)?;

        Ok(other == git2_oid || is_descendant)
    }

    pub(crate) fn diff_commit_and_parents<P>(
        &self,
        path: &P,
        commit: &git2::Commit,
    ) -> Result<Option<PathBuf>, Error>
    where
        P: AsRef<Path>,
    {
        let mut parents = commit.parents();

        let diff = self.diff_commits(Some(path.as_ref()), parents.next().as_ref(), commit)?;
        if let Some(_delta) = diff.deltas().next() {
            Ok(Some(path.as_ref().to_path_buf()))
        } else {
            Ok(None)
        }
    }

    /// Create a diff with the difference between two tree objects.
    ///
    /// Defines some options and flags that are passed to git2.
    ///
    /// Note:
    /// libgit2 optimizes around not loading the content when there's no content
    /// callbacks configured. Be aware that binaries aren't detected as
    /// expected.
    ///
    /// Reference: <https://github.com/libgit2/libgit2/issues/6637>
    fn diff_commits<'a>(
        &'a self,
        path: Option<&Path>,
        from: Option<&git2::Commit>,
        to: &git2::Commit,
    ) -> Result<git2::Diff<'a>, Error> {
        let new_tree = to.tree()?;
        let old_tree = from.map_or(Ok(None), |c| c.tree().map(Some))?;

        let mut opts = git2::DiffOptions::new();
        if let Some(path) = path {
            opts.pathspec(path.to_string_lossy().to_string());
            opts.disable_pathspec_match(true);
            opts.skip_binary_check(false);
        }

        let mut diff =
            self.inner
                .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

        // Detect renames by default.
        let mut find_opts = git2::DiffFindOptions::new();
        find_opts.renames(true);
        find_opts.copies(true);
        diff.find_similar(Some(&mut find_opts))?;

        Ok(diff)
    }

    /// Returns a full reference name with namespace(s) included.
    pub(crate) fn namespaced_refname<'a>(
        &'a self,
        refname: &Qualified<'a>,
    ) -> Result<Qualified<'a>, Error> {
        let fullname = match self.which_namespace()? {
            Some(namespace) => namespace.to_namespaced(refname).into_qualified(),
            None => refname.clone(),
        };
        Ok(fullname)
    }

    /// Returns a full reference name with namespace(s) included.
    fn namespaced_pattern<'a>(
        &'a self,
        refname: &QualifiedPattern<'a>,
    ) -> Result<QualifiedPattern<'a>, Error> {
        let fullname = match self.which_namespace()? {
            Some(namespace) => namespace.to_namespaced_pattern(refname).into_qualified(),
            None => refname.clone(),
        };
        Ok(fullname)
    }
}

impl From<git2::Repository> for Repository {
    fn from(repo: git2::Repository) -> Self {
        Repository { inner: repo }
    }
}

impl std::fmt::Debug for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".git")
    }
}

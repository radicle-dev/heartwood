use std::marker::PhantomData;

use git_ext::ref_format::{
    self, refname,
    refspec::{self, PatternString, QualifiedPattern},
    Qualified, RefStr, RefString,
};
use thiserror::Error;

use crate::{Branch, Local, Namespace, Remote, Tag};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    RefFormat(#[from] ref_format::Error),
}

/// A collection of globs for a git reference type.
#[derive(Clone, Debug)]
pub struct Glob<T> {
    globs: Vec<QualifiedPattern<'static>>,
    glob_type: PhantomData<T>, // To support different methods for different T.
}

impl<T> Default for Glob<T> {
    fn default() -> Self {
        Self {
            globs: Default::default(),
            glob_type: PhantomData,
        }
    }
}

impl<T> Glob<T> {
    /// Return the [`QualifiedPattern`] globs of this `Glob`.
    pub fn globs(&self) -> impl Iterator<Item = &QualifiedPattern<'static>> {
        self.globs.iter()
    }

    /// Combine two `Glob`s together by combining their glob lists together.
    ///
    /// Note that the `Glob`s must result in the same type,
    /// e.g. `Glob<Tag>` can only combine with `Glob<Tag>`,
    /// `Glob<Local>` can combine with `Glob<Remote>`, etc.
    pub fn and(mut self, other: impl Into<Self>) -> Self {
        self.globs.extend(other.into().globs);
        self
    }
}

impl Glob<Namespace> {
    /// Creates the `Glob` that matches all `refs/namespaces`.
    pub fn all_namespaces() -> Self {
        Self::namespaces(refspec::pattern!("*"))
    }

    /// Creates a `Glob` for `refs/namespaces`, starting with `glob`.
    pub fn namespaces(glob: PatternString) -> Self {
        let globs = vec![Self::qualify(glob)];
        Self {
            globs,
            glob_type: PhantomData,
        }
    }

    /// Adds a `refs/namespaces` pattern to this `Glob`.
    pub fn insert(mut self, glob: PatternString) -> Self {
        self.globs.push(Self::qualify(glob));
        self
    }

    fn qualify(glob: PatternString) -> QualifiedPattern<'static> {
        qualify(&refname!("refs/namespaces"), glob).expect("BUG: pattern is qualified")
    }
}

impl FromIterator<PatternString> for Glob<Namespace> {
    fn from_iter<T: IntoIterator<Item = PatternString>>(iter: T) -> Self {
        let globs = iter
            .into_iter()
            .map(|pat| {
                qualify(&refname!("refs/namespaces"), pat).expect("BUG: pattern is qualified")
            })
            .collect();

        Self {
            globs,
            glob_type: PhantomData,
        }
    }
}

impl Extend<PatternString> for Glob<Namespace> {
    fn extend<T: IntoIterator<Item = PatternString>>(&mut self, iter: T) {
        self.globs.extend(iter.into_iter().map(|pat| {
            qualify(&refname!("refs/namespaces"), pat).expect("BUG: pattern is qualified")
        }))
    }
}

impl Glob<Tag> {
    /// Creates a `Glob` that matches all `refs/tags`.
    pub fn all_tags() -> Self {
        Self::tags(refspec::pattern!("*"))
    }

    /// Creates a `Glob` for `refs/tags`, starting with `glob`.
    pub fn tags(glob: PatternString) -> Self {
        let globs = vec![Self::qualify(glob)];
        Self {
            globs,
            glob_type: PhantomData,
        }
    }

    /// Adds a `refs/tags` pattern to this `Glob`.
    pub fn insert(mut self, glob: PatternString) -> Self {
        self.globs.push(Self::qualify(glob));
        self
    }

    fn qualify(glob: PatternString) -> QualifiedPattern<'static> {
        qualify(&refname!("refs/tags"), glob).expect("BUG: pattern is qualified")
    }
}

impl FromIterator<PatternString> for Glob<Tag> {
    fn from_iter<T: IntoIterator<Item = PatternString>>(iter: T) -> Self {
        let globs = iter
            .into_iter()
            .map(|pat| qualify(&refname!("refs/tags"), pat).expect("BUG: pattern is qualified"))
            .collect();

        Self {
            globs,
            glob_type: PhantomData,
        }
    }
}

impl Extend<PatternString> for Glob<Tag> {
    fn extend<T: IntoIterator<Item = PatternString>>(&mut self, iter: T) {
        self.globs.extend(
            iter.into_iter()
                .map(|pat| qualify(&refname!("refs/tag"), pat).expect("BUG: pattern is qualified")),
        )
    }
}

impl Glob<Local> {
    /// Creates the `Glob` that matches all `refs/heads`.
    pub fn all_heads() -> Self {
        Self::heads(refspec::pattern!("*"))
    }

    /// Creates a `Glob` for `refs/heads`, starting with `glob`.
    pub fn heads(glob: PatternString) -> Self {
        let globs = vec![Self::qualify_heads(glob)];
        Self {
            globs,
            glob_type: PhantomData,
        }
    }

    /// Adds a `refs/heads` pattern to this `Glob`.
    pub fn insert(mut self, glob: PatternString) -> Self {
        self.globs.push(Self::qualify_heads(glob));
        self
    }

    /// When chaining `Glob<Local>` with `Glob<Remote>`, use
    /// `branches` to convert this `Glob<Local>` into a
    /// `Glob<Branch>`.
    ///
    /// # Example
    /// ```no_run
    /// Glob::heads(pattern!("features/*"))
    ///     .insert(pattern!("qa/*"))
    ///     .branches()
    ///     .and(Glob::remotes(pattern!("origin/features/*")))
    /// ```
    pub fn branches(self) -> Glob<Branch> {
        self.into()
    }

    fn qualify_heads(glob: PatternString) -> QualifiedPattern<'static> {
        qualify(&refname!("refs/heads"), glob).expect("BUG: pattern is qualified")
    }
}

impl FromIterator<PatternString> for Glob<Local> {
    fn from_iter<T: IntoIterator<Item = PatternString>>(iter: T) -> Self {
        let globs = iter
            .into_iter()
            .map(|pat| qualify(&refname!("refs/heads"), pat).expect("BUG: pattern is qualified"))
            .collect();

        Self {
            globs,
            glob_type: PhantomData,
        }
    }
}

impl Extend<PatternString> for Glob<Local> {
    fn extend<T: IntoIterator<Item = PatternString>>(&mut self, iter: T) {
        self.globs.extend(
            iter.into_iter().map(|pat| {
                qualify(&refname!("refs/heads"), pat).expect("BUG: pattern is qualified")
            }),
        )
    }
}

impl From<Glob<Local>> for Glob<Branch> {
    fn from(Glob { globs, .. }: Glob<Local>) -> Self {
        Self {
            globs,
            glob_type: PhantomData,
        }
    }
}

impl Glob<Remote> {
    /// Creates the `Glob` that matches all `refs/remotes`.
    pub fn all_remotes() -> Self {
        Self::remotes(refspec::pattern!("*"))
    }

    /// Creates a `Glob` for `refs/remotes`, starting with `glob`.
    pub fn remotes(glob: PatternString) -> Self {
        let globs = vec![Self::qualify_remotes(glob)];
        Self {
            globs,
            glob_type: PhantomData,
        }
    }

    /// Adds a `refs/remotes` pattern to this `Glob`.
    pub fn insert(mut self, glob: PatternString) -> Self {
        self.globs.push(Self::qualify_remotes(glob));
        self
    }

    /// When chaining `Glob<Remote>` with `Glob<Local>`, use
    /// `branches` to convert this `Glob<Remote>` into a
    /// `Glob<Branch>`.
    ///
    /// # Example
    /// ```no_run
    /// Glob::remotes(pattern!("origin/features/*"))
    ///     .insert(pattern!("origin/qa/*"))
    ///     .branches()
    ///     .and(Glob::heads(pattern!("features/*")))
    /// ```
    pub fn branches(self) -> Glob<Branch> {
        self.into()
    }

    fn qualify_remotes(glob: PatternString) -> QualifiedPattern<'static> {
        qualify(&refname!("refs/remotes"), glob).expect("BUG: pattern is qualified")
    }
}

impl FromIterator<PatternString> for Glob<Remote> {
    fn from_iter<T: IntoIterator<Item = PatternString>>(iter: T) -> Self {
        let globs = iter
            .into_iter()
            .map(|pat| qualify(&refname!("refs/remotes"), pat).expect("BUG: pattern is qualified"))
            .collect();

        Self {
            globs,
            glob_type: PhantomData,
        }
    }
}

impl Extend<PatternString> for Glob<Remote> {
    fn extend<T: IntoIterator<Item = PatternString>>(&mut self, iter: T) {
        self.globs.extend(
            iter.into_iter().map(|pat| {
                qualify(&refname!("refs/remotes"), pat).expect("BUG: pattern is qualified")
            }),
        )
    }
}

impl From<Glob<Remote>> for Glob<Branch> {
    fn from(Glob { globs, .. }: Glob<Remote>) -> Self {
        Self {
            globs,
            glob_type: PhantomData,
        }
    }
}

impl Glob<Qualified<'_>> {
    pub fn all_category<R: AsRef<RefStr>>(category: R) -> Self {
        Self {
            globs: vec![Self::qualify_category(category, refspec::pattern!("*"))],
            glob_type: PhantomData,
        }
    }

    /// Creates a `Glob` for `refs/<category>`, starting with `glob`.
    pub fn categories<R>(category: R, glob: PatternString) -> Self
    where
        R: AsRef<RefStr>,
    {
        let globs = vec![Self::qualify_category(category, glob)];
        Self {
            globs,
            glob_type: PhantomData,
        }
    }

    /// Adds a `refs/<category>` pattern to this `Glob`.
    pub fn insert<R>(mut self, category: R, glob: PatternString) -> Self
    where
        R: AsRef<RefStr>,
    {
        self.globs.push(Self::qualify_category(category, glob));
        self
    }

    fn qualify_category<R>(category: R, glob: PatternString) -> QualifiedPattern<'static>
    where
        R: AsRef<RefStr>,
    {
        let prefix = refname!("refs").and(category);
        qualify(&prefix, glob).expect("BUG: pattern is qualified")
    }
}

fn qualify(prefix: &RefString, glob: PatternString) -> Option<QualifiedPattern<'static>> {
    prefix.to_pattern(glob).qualified().map(|q| q.into_owned())
}

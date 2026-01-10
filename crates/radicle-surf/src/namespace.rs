use std::{
    convert::TryFrom,
    fmt,
    str::{self, FromStr},
};

use git_ext::ref_format::{
    self,
    refspec::{NamespacedPattern, PatternString, QualifiedPattern},
    Component, Namespaced, Qualified, RefStr, RefString,
};
use nonempty::NonEmpty;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// When parsing a namespace we may come across one that was an empty
    /// string.
    #[error("namespaces must not be empty")]
    EmptyNamespace,
    #[error(transparent)]
    RefFormat(#[from] ref_format::Error),
    #[error(transparent)]
    Utf8(#[from] str::Utf8Error),
}

/// A `Namespace` value allows us to switch the git namespace of
/// a repo.
///
/// A `Namespace` is one or more name components separated by `/`, e.g. `surf`,
/// `surf/git`.
///
/// For each `Namespace`, the reference name will add a single `refs/namespaces`
/// prefix, e.g. `refs/namespaces/surf`,
/// `refs/namespaces/surf/refs/namespaces/git`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Namespace {
    // XXX: we rely on RefString being non-empty here, which
    // git-ref-format ensures that there's no way to construct one.
    pub(super) namespaces: RefString,
}

impl Namespace {
    /// Take a `Qualified` reference name and convert it to a `Namespaced` using
    /// this `Namespace`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let ns = "surf/git".parse::<Namespace>();
    /// let name = ns.to_namespaced(qualified!("refs/heads/main"));
    /// assert_eq!(
    ///     name.as_str(),
    ///     "refs/namespaces/surf/refs/namespaces/git/refs/heads/main"
    /// );
    /// ```
    pub(crate) fn to_namespaced<'a>(&self, name: &Qualified<'a>) -> Namespaced<'a> {
        let mut components = self.namespaces.components().rev();
        let mut namespaced = name.with_namespace(
            components
                .next()
                .expect("BUG: 'namespaces' cannot be empty"),
        );
        for ns in components {
            let qualified = namespaced.into_qualified();
            namespaced = qualified.with_namespace(ns);
        }
        namespaced
    }

    /// Take a `QualifiedPattern` reference name and convert it to a
    /// `NamespacedPattern` using this `Namespace`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let ns = "surf/git".parse::<Namespace>();
    /// let name = ns.to_namespaced(pattern!("refs/heads/*").to_qualified().unwrap());
    /// assert_eq!(
    ///     name.as_str(),
    ///     "refs/namespaces/surf/refs/namespaces/git/refs/heads/*"
    /// );
    /// ```
    pub(crate) fn to_namespaced_pattern<'a>(
        &self,
        pat: &QualifiedPattern<'a>,
    ) -> NamespacedPattern<'a> {
        let pattern = PatternString::from(self.namespaces.clone());
        let mut components = pattern.components().rev();
        let mut namespaced = pat
            .with_namespace(
                components
                    .next()
                    .expect("BUG: 'namespaces' cannot be empty"),
            )
            .expect("BUG: 'namespace' cannot have globs");
        for ns in components {
            let qualified = namespaced.into_qualified();
            namespaced = qualified
                .with_namespace(ns)
                .expect("BUG: 'namespaces' cannot have globs");
        }
        namespaced
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.namespaces)
    }
}

impl<'a> From<NonEmpty<Component<'a>>> for Namespace {
    fn from(cs: NonEmpty<Component<'a>>) -> Self {
        Self {
            namespaces: cs.into_iter().collect::<RefString>(),
        }
    }
}

impl TryFrom<&str> for Namespace {
    type Error = Error;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        Self::from_str(name)
    }
}

impl TryFrom<&[u8]> for Namespace {
    type Error = Error;

    fn try_from(namespace: &[u8]) -> Result<Self, Self::Error> {
        str::from_utf8(namespace)
            .map_err(Error::from)
            .and_then(Self::from_str)
    }
}

impl FromStr for Namespace {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let namespaces = RefStr::try_from_str(name)?.to_ref_string();
        Ok(Self { namespaces })
    }
}

impl From<Namespaced<'_>> for Namespace {
    fn from(namespaced: Namespaced<'_>) -> Self {
        let mut namespaces = namespaced.namespace().to_ref_string();
        let mut qualified = namespaced.strip_namespace();
        while let Some(namespaced) = qualified.to_namespaced() {
            namespaces.push(namespaced.namespace());
            qualified = namespaced.strip_namespace();
        }
        Self { namespaces }
    }
}

impl TryFrom<&git2::Reference<'_>> for Namespace {
    type Error = Error;

    fn try_from(reference: &git2::Reference) -> Result<Self, Self::Error> {
        let name = RefStr::try_from_str(str::from_utf8(reference.name_bytes())?)?;
        name.to_namespaced()
            .ok_or(Error::EmptyNamespace)
            .map(Self::from)
    }
}

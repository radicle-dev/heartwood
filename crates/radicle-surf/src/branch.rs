use std::{
    convert::TryFrom,
    str::{self, FromStr},
};

use git_ext::ref_format::{component, lit, Component, Qualified, RefStr, RefString};

use crate::refs::refstr_join;

/// A `Branch` represents any git branch. It can be [`Local`] or [`Remote`].
///
/// Note that if a `Branch` is created from a [`git2::Reference`] then
/// any `refs/namespaces` will be stripped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Branch {
    Local(Local),
    Remote(Remote),
}

impl Branch {
    /// Construct a [`Local`] branch.
    pub fn local<R>(name: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self::Local(Local::new(name))
    }

    /// Construct a [`Remote`] branch.
    /// The `remote` is the remote name of the reference name while
    /// the `name` is the suffix, i.e. `refs/remotes/<remote>/<name>`.
    pub fn remote<R>(remote: Component<'_>, name: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self::Remote(Remote::new(remote, name))
    }

    /// Return the short `Branch` refname,
    /// e.g. `fix/ref-format`.
    pub fn short_name(&self) -> &RefString {
        match self {
            Branch::Local(local) => local.short_name(),
            Branch::Remote(remote) => remote.short_name(),
        }
    }

    /// Give back the fully qualified `Branch` refname,
    /// e.g. `refs/remotes/origin/fix/ref-format`,
    /// `refs/heads/fix/ref-format`.
    pub fn refname<'a>(&'a self) -> Qualified<'a> {
        match self {
            Branch::Local(local) => local.refname(),
            Branch::Remote(remote) => remote.refname(),
        }
    }
}

impl TryFrom<&git2::Reference<'_>> for Branch {
    type Error = error::Branch;

    fn try_from(reference: &git2::Reference<'_>) -> Result<Self, Self::Error> {
        let name = str::from_utf8(reference.name_bytes())?;
        Self::from_str(name)
    }
}

impl TryFrom<&str> for Branch {
    type Error = error::Branch;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        Self::from_str(name)
    }
}

impl FromStr for Branch {
    type Err = error::Branch;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let name = RefStr::try_from_str(name)?;
        let name = match name.to_namespaced() {
            None => name
                .qualified()
                .ok_or_else(|| error::Branch::NotQualified(name.to_string()))?,
            Some(name) => name.strip_namespace_recursive(),
        };

        let (_ref, category, c, cs) = name.non_empty_components();

        if category == component::HEADS {
            Ok(Self::Local(Local::new(refstr_join(c, cs))))
        } else if category == component::REMOTES {
            Ok(Self::Remote(Remote::new(c, cs.collect::<RefString>())))
        } else {
            Err(error::Branch::InvalidName(name.into()))
        }
    }
}

/// A `Local` represents a local branch, i.e. it is a reference under
/// `refs/heads`.
///
/// Note that if a `Local` is created from a [`git2::Reference`] then
/// any `refs/namespaces` will be stripped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Local {
    name: RefString,
}

impl Local {
    /// Construct a new `Local` with the given `name`.
    ///
    /// If the name is qualified with `refs/heads`, this will be
    /// shortened to the suffix. To get the `Qualified` name again,
    /// use [`Local::refname`].
    pub(crate) fn new<R>(name: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        match name.as_ref().qualified() {
            None => Self {
                name: name.as_ref().to_ref_string(),
            },
            Some(qualified) => {
                let (_refs, heads, c, cs) = qualified.non_empty_components();
                if heads == component::HEADS {
                    Self {
                        name: refstr_join(c, cs),
                    }
                } else {
                    Self {
                        name: name.as_ref().to_ref_string(),
                    }
                }
            }
        }
    }

    /// Return the short `Local` refname,
    /// e.g. `fix/ref-format`.
    pub fn short_name(&self) -> &RefString {
        &self.name
    }

    /// Return the fully qualified `Local` refname,
    /// e.g. `refs/heads/fix/ref-format`.
    pub fn refname<'a>(&'a self) -> Qualified<'a> {
        lit::refs_heads(&self.name).into()
    }
}

impl TryFrom<&git2::Reference<'_>> for Local {
    type Error = error::Local;

    fn try_from(reference: &git2::Reference) -> Result<Self, Self::Error> {
        let name = str::from_utf8(reference.name_bytes())?;
        Self::from_str(name)
    }
}

impl TryFrom<&str> for Local {
    type Error = error::Local;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        Self::from_str(name)
    }
}

impl FromStr for Local {
    type Err = error::Local;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let name = RefStr::try_from_str(name)?;
        let name = match name.to_namespaced() {
            None => name
                .qualified()
                .ok_or_else(|| error::Local::NotQualified(name.to_string()))?,
            Some(name) => name.strip_namespace_recursive(),
        };

        let (_ref, heads, c, cs) = name.non_empty_components();
        if heads == component::HEADS {
            Ok(Self::new(refstr_join(c, cs)))
        } else {
            Err(error::Local::NotHeads(name.into()))
        }
    }
}

/// A `Remote` represents a remote branch, i.e. it is a reference under
/// `refs/remotes`.
///
/// Note that if a `Remote` is created from a [`git2::Reference`] then
/// any `refs/namespaces` will be stripped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Remote {
    remote: RefString,
    name: RefString,
}

impl Remote {
    /// Construct a new `Remote` with the given `name` and `remote`.
    ///
    /// ## Note
    /// `name` is expected to be in short form, i.e. not begin with
    /// `refs`.
    ///
    /// If you are creating a `Remote` with a name that begins with
    /// `refs/remotes`, use [`Remote::from_refs_remotes`] instead.
    ///
    /// To get the `Qualified` name, use [`Remote::refname`].
    pub(crate) fn new<R>(remote: Component, name: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        Self {
            name: name.as_ref().to_ref_string(),
            remote: remote.to_ref_string(),
        }
    }

    /// Parse the `name` from the form `refs/remotes/<remote>/<rest>`.
    ///
    /// If the `name` is not of this form, then `None` is returned.
    pub fn from_refs_remotes<R>(name: R) -> Option<Self>
    where
        R: AsRef<RefStr>,
    {
        let qualified = name.as_ref().qualified()?;
        let (_refs, remotes, remote, cs) = qualified.non_empty_components();
        (remotes == component::REMOTES).then_some(Self {
            name: cs.collect(),
            remote: remote.to_ref_string(),
        })
    }

    /// Return the short `Remote` refname,
    /// e.g. `fix/ref-format`.
    pub fn short_name(&self) -> &RefString {
        &self.name
    }

    /// Return the remote of the `Remote`'s refname,
    /// e.g. `origin`.
    pub fn remote(&self) -> &RefString {
        &self.remote
    }

    /// Give back the fully qualified `Remote` refname,
    /// e.g. `refs/remotes/origin/fix/ref-format`.
    pub fn refname<'a>(&'a self) -> Qualified<'a> {
        lit::refs_remotes(self.remote.join(&self.name)).into()
    }
}

impl TryFrom<&git2::Reference<'_>> for Remote {
    type Error = error::Remote;

    fn try_from(reference: &git2::Reference) -> Result<Self, Self::Error> {
        let name = str::from_utf8(reference.name_bytes())?;
        Self::from_str(name)
    }
}

impl TryFrom<&str> for Remote {
    type Error = error::Remote;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        Self::from_str(name)
    }
}

impl FromStr for Remote {
    type Err = error::Remote;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let name = RefStr::try_from_str(name)?;
        let name = match name.to_namespaced() {
            None => name
                .qualified()
                .ok_or_else(|| error::Remote::NotQualified(name.to_string()))?,
            Some(name) => name.strip_namespace_recursive(),
        };

        let (_ref, remotes, remote, cs) = name.non_empty_components();
        if remotes == component::REMOTES {
            Ok(Self::new(remote, cs.collect::<RefString>()))
        } else {
            Err(error::Remote::NotRemotes(name.into()))
        }
    }
}

pub mod error {
    use radicle_git_ext::ref_format::{self, RefString};
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Branch {
        #[error("the refname '{0}' did not begin with 'refs/heads' or 'refs/remotes'")]
        InvalidName(RefString),
        #[error("the refname '{0}' did not begin with 'refs/heads' or 'refs/remotes'")]
        NotQualified(String),
        #[error(transparent)]
        RefFormat(#[from] ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] std::str::Utf8Error),
    }

    #[derive(Debug, Error)]
    pub enum Local {
        #[error("the refname '{0}' did not begin with 'refs/heads'")]
        NotHeads(RefString),
        #[error("the refname '{0}' did not begin with 'refs/heads'")]
        NotQualified(String),
        #[error(transparent)]
        RefFormat(#[from] ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] std::str::Utf8Error),
    }

    #[derive(Debug, Error)]
    pub enum Remote {
        #[error("the refname '{0}' did not begin with 'refs/remotes'")]
        NotQualified(String),
        #[error("the refname '{0}' did not begin with 'refs/remotes'")]
        NotRemotes(RefString),
        #[error(transparent)]
        RefFormat(#[from] ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] std::str::Utf8Error),
    }
}

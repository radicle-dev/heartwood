// I think the following `Tags` and `Branches` would be merged
// using Generic associated types supported in Rust 1.65.0.

use std::{
    collections::{btree_set, BTreeSet},
    convert::TryFrom as _,
};

use git_ext::ref_format::{self, lit, name::Components, Component, Qualified, RefString};

use crate::{tag, Branch, Namespace, Tag};

/// Iterator over [`Tag`]s.
#[derive(Default)]
pub struct Tags<'a> {
    references: Vec<git2::References<'a>>,
    current: usize,
}

/// Iterator over the [`Qualified`] names of [`Tag`]s.
pub struct TagNames<'a> {
    inner: Tags<'a>,
}

impl<'a> Tags<'a> {
    pub(super) fn push(&mut self, references: git2::References<'a>) {
        self.references.push(references)
    }

    pub fn names(self) -> TagNames<'a> {
        TagNames { inner: self }
    }
}

impl Iterator for Tags<'_> {
    type Item = Result<Tag, error::Tag>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.references.len() {
            match self.references.get_mut(self.current) {
                Some(refs) => match refs.next() {
                    Some(res) => {
                        return Some(
                            res.map_err(error::Tag::from)
                                .and_then(|r| Tag::try_from(&r).map_err(error::Tag::from)),
                        );
                    }
                    None => self.current += 1,
                },
                None => break,
            }
        }
        None
    }
}

impl Iterator for TagNames<'_> {
    type Item = Result<Qualified<'static>, error::Tag>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.inner.current < self.inner.references.len() {
            match self.inner.references.get_mut(self.inner.current) {
                Some(refs) => match refs.next() {
                    Some(res) => {
                        return Some(res.map_err(error::Tag::from).and_then(|r| {
                            tag::reference_name(&r)
                                .map(|name| lit::refs_tags(name).into())
                                .map_err(error::Tag::from)
                        }))
                    }
                    None => self.inner.current += 1,
                },
                None => break,
            }
        }
        None
    }
}

/// Iterator over [`Branch`]es.
#[derive(Default)]
pub struct Branches<'a> {
    references: Vec<git2::References<'a>>,
    current: usize,
}

/// Iterator over the [`Qualified`] names of [`Branch`]es.
pub struct BranchNames<'a> {
    inner: Branches<'a>,
}

impl<'a> Branches<'a> {
    pub(super) fn push(&mut self, references: git2::References<'a>) {
        self.references.push(references)
    }

    pub fn names(self) -> BranchNames<'a> {
        BranchNames { inner: self }
    }
}

impl Iterator for Branches<'_> {
    type Item = Result<Branch, error::Branch>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.references.len() {
            match self.references.get_mut(self.current) {
                Some(refs) => match refs.next() {
                    Some(res) => {
                        return Some(
                            res.map_err(error::Branch::from)
                                .and_then(|r| Branch::try_from(&r).map_err(error::Branch::from)),
                        )
                    }
                    None => self.current += 1,
                },
                None => break,
            }
        }
        None
    }
}

impl Iterator for BranchNames<'_> {
    type Item = Result<Qualified<'static>, error::Branch>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.inner.current < self.inner.references.len() {
            match self.inner.references.get_mut(self.inner.current) {
                Some(refs) => match refs.next() {
                    Some(res) => {
                        return Some(res.map_err(error::Branch::from).and_then(|r| {
                            Branch::try_from(&r)
                                .map(|branch| branch.refname().into_owned())
                                .map_err(error::Branch::from)
                        }))
                    }
                    None => self.inner.current += 1,
                },
                None => break,
            }
        }
        None
    }
}

// TODO: not sure this buys us much
/// An iterator for namespaces.
pub struct Namespaces {
    namespaces: btree_set::IntoIter<Namespace>,
}

impl Namespaces {
    pub(super) fn new(namespaces: BTreeSet<Namespace>) -> Self {
        Self {
            namespaces: namespaces.into_iter(),
        }
    }
}

impl Iterator for Namespaces {
    type Item = Namespace;
    fn next(&mut self) -> Option<Self::Item> {
        self.namespaces.next()
    }
}

#[derive(Default)]
pub struct Categories<'a> {
    references: Vec<git2::References<'a>>,
    current: usize,
}

impl<'a> Categories<'a> {
    pub(super) fn push(&mut self, references: git2::References<'a>) {
        self.references.push(references)
    }
}

impl Iterator for Categories<'_> {
    type Item = Result<(RefString, RefString), error::Category>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.references.len() {
            match self.references.get_mut(self.current) {
                Some(refs) => match refs.next() {
                    Some(res) => {
                        return Some(res.map_err(error::Category::from).and_then(|r| {
                            let name = std::str::from_utf8(r.name_bytes())?;
                            let name = ref_format::RefStr::try_from_str(name)?;
                            let name = name.qualified().ok_or_else(|| {
                                error::Category::NotQualified(name.to_ref_string())
                            })?;
                            let (_refs, category, c, cs) = name.non_empty_components();
                            Ok((category.to_ref_string(), refstr_join(c, cs)))
                        }));
                    }
                    None => self.current += 1,
                },
                None => break,
            }
        }
        None
    }
}

pub mod error {
    use std::str;

    use radicle_git_ext::ref_format::{self, RefString};
    use thiserror::Error;

    use crate::{branch, tag};

    #[derive(Debug, Error)]
    pub enum Branch {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Branch(#[from] branch::error::Branch),
    }

    #[derive(Debug, Error)]
    pub enum Category {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("the reference '{0}' was expected to be qualified, i.e. 'refs/<category>/<path>'")]
        NotQualified(RefString),
        #[error(transparent)]
        RefFormat(#[from] ref_format::Error),
        #[error(transparent)]
        Utf8(#[from] str::Utf8Error),
    }

    #[derive(Debug, Error)]
    pub enum Tag {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Tag(#[from] tag::error::FromReference),
    }
}

pub(crate) fn refstr_join<'a>(c: Component<'a>, cs: Components<'a>) -> RefString {
    std::iter::once(c).chain(cs).collect::<RefString>()
}

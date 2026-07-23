use crate::{name, Qualified, RefStr};

/// A literal [`RefStr`].
///
/// Types implementing [`Lit`] must be [`name::Component`]s, and provide a
/// conversion from a component _iff_ the component's [`RefStr`] representation
/// is equal to [`Lit::NAME`]. Because these morphisms can only be guaranteed
/// axiomatically, the trait can not currently be implemented by types outside
/// of this crate.
///
/// [`Lit`] types are useful for efficiently creating known-valid [`Qualified`]
/// refs, and sometimes for pattern matching.
pub trait Lit: Sized + sealed::Sealed {
    const SELF: Self;
    const NAME: &'static RefStr;

    #[inline]
    fn from_component(c: &name::Component) -> Option<Self> {
        (c.as_ref() == Self::NAME).then_some(Self::SELF)
    }
}

impl<T: Lit> From<T> for &'static RefStr {
    #[inline]
    fn from(_: T) -> Self {
        T::NAME
    }
}

mod sealed {
    pub trait Sealed {}
}

/// All known literal [`RefStr`]s.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum KnownLit {
    Refs,
    Heads,
    Namespaces,
    Remotes,
    Tags,
    Notes,
}

impl KnownLit {
    #[inline]
    pub fn from_component(c: &name::Component) -> Option<Self> {
        let c: &RefStr = c.as_ref();
        if c == Refs::NAME {
            Some(Self::Refs)
        } else if c == Heads::NAME {
            Some(Self::Heads)
        } else if c == Namespaces::NAME {
            Some(Self::Namespaces)
        } else if c == Remotes::NAME {
            Some(Self::Remotes)
        } else if c == Tags::NAME {
            Some(Self::Tags)
        } else if c == Notes::NAME {
            Some(Self::Notes)
        } else {
            None
        }
    }
}

impl From<KnownLit> for name::Component<'_> {
    #[inline]
    fn from(k: KnownLit) -> Self {
        match k {
            KnownLit::Refs => Refs.into(),
            KnownLit::Heads => Heads.into(),
            KnownLit::Namespaces => Namespaces.into(),
            KnownLit::Remotes => Remotes.into(),
            KnownLit::Tags => Tags.into(),
            KnownLit::Notes => Notes.into(),
        }
    }
}

/// Either a [`KnownLit`] or a [`name::Component`]
pub enum SomeLit<'a> {
    Known(KnownLit),
    Any(name::Component<'a>),
}

impl SomeLit<'_> {
    pub fn known(self) -> Option<KnownLit> {
        match self {
            Self::Known(k) => Some(k),
            _ => None,
        }
    }
}

impl<'a> From<name::Component<'a>> for SomeLit<'a> {
    #[inline]
    fn from(c: name::Component<'a>) -> Self {
        match KnownLit::from_component(&c) {
            Some(k) => Self::Known(k),
            None => Self::Any(c),
        }
    }
}

pub type RefsHeads<T> = (Refs, Heads, T);
pub type RefsTags<T> = (Refs, Tags, T);
pub type RefsNotes<T> = (Refs, Notes, T);
pub type RefsRemotes<T> = (Refs, Remotes, T);
pub type RefsNamespaces<'a, T> = (Refs, Namespaces, T, Qualified<'a>);

#[inline]
pub fn refs_heads<T: AsRef<RefStr>>(name: T) -> RefsHeads<T> {
    (Refs, Heads, name)
}

#[inline]
pub fn refs_tags<T: AsRef<RefStr>>(name: T) -> RefsTags<T> {
    (Refs, Tags, name)
}

#[inline]
pub fn refs_notes<T: AsRef<RefStr>>(name: T) -> RefsNotes<T> {
    (Refs, Notes, name)
}

#[inline]
pub fn refs_remotes<T: AsRef<RefStr>>(name: T) -> RefsRemotes<T> {
    (Refs, Remotes, name)
}

#[inline]
pub fn refs_namespaces<'a, 'b, T>(namespace: T, name: Qualified<'b>) -> RefsNamespaces<'b, T>
where
    T: Into<name::Component<'a>>,
{
    (Refs, Namespaces, namespace, name)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Refs;

impl Lit for Refs {
    const SELF: Self = Self;
    const NAME: &'static RefStr = name::REFS;
}
impl sealed::Sealed for Refs {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Heads;

impl Lit for Heads {
    const SELF: Self = Self;
    const NAME: &'static RefStr = name::HEADS;
}
impl sealed::Sealed for Heads {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Namespaces;

impl Lit for Namespaces {
    const SELF: Self = Self;
    const NAME: &'static RefStr = name::NAMESPACES;
}
impl sealed::Sealed for Namespaces {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Remotes;

impl Lit for Remotes {
    const SELF: Self = Self;
    const NAME: &'static RefStr = name::REMOTES;
}
impl sealed::Sealed for Remotes {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Tags;

impl Lit for Tags {
    const SELF: Self = Self;
    const NAME: &'static RefStr = name::TAGS;
}
impl sealed::Sealed for Tags {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Notes;

impl Lit for Notes {
    const SELF: Self = Self;
    const NAME: &'static RefStr = name::NOTES;
}
impl sealed::Sealed for Notes {}

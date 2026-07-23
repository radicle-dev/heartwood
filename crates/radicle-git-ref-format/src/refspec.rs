use std::{
    borrow::{Borrow, Cow},
    convert::TryFrom,
    fmt::{self, Display, Write as _},
    iter::FromIterator,
    ops::Deref,
};

use thiserror::Error;

use crate::{check, lit, Namespaced, Qualified, RefStr, RefString};

mod iter;
pub use iter::{component, Component, Components, Iter};

pub const STAR: &PatternStr = PatternStr::from_str("*");

const CHECK_OPTS: check::Options = check::Options {
    allow_onelevel: true,
    allow_pattern: true,
};

#[repr(transparent)]
#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct PatternStr(str);

impl PatternStr {
    #[inline]
    pub fn try_from_str(s: &str) -> Result<&Self, check::Error> {
        TryFrom::try_from(s)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self
    }

    pub fn join<R>(&self, other: R) -> PatternString
    where
        R: AsRef<RefStr>,
    {
        self._join(other.as_ref())
    }

    fn _join(&self, other: &RefStr) -> PatternString {
        let mut buf = self.to_owned();
        buf.push(other);
        buf
    }

    #[inline]
    pub fn qualified<'a>(&'a self) -> Option<QualifiedPattern<'a>> {
        QualifiedPattern::from_patternstr(self)
    }

    #[inline]
    pub fn to_namespaced<'a>(&'a self) -> Option<NamespacedPattern<'a>> {
        self.into()
    }

    #[inline]
    pub fn iter<'a>(&'a self) -> Iter<'a> {
        self.0.split('/')
    }

    #[inline]
    pub fn components<'a>(&'a self) -> Components<'a> {
        Components::from(self)
    }

    pub(crate) const fn from_str(s: &str) -> &PatternStr {
        unsafe { &*(s as *const str as *const PatternStr) }
    }
}

impl Deref for PatternStr {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for PatternStr {
    #[inline]
    fn as_ref(&self) -> &str {
        self
    }
}

impl AsRef<Self> for PatternStr {
    #[inline]
    fn as_ref(&self) -> &Self {
        self
    }
}

impl<'a> TryFrom<&'a str> for &'a PatternStr {
    type Error = check::Error;

    #[inline]
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        check::ref_format(CHECK_OPTS, s).map(|()| PatternStr::from_str(s))
    }
}

impl<'a> From<&'a RefStr> for &'a PatternStr {
    #[inline]
    fn from(rs: &'a RefStr) -> Self {
        PatternStr::from_str(rs.as_str())
    }
}

impl<'a> From<&'a PatternStr> for Cow<'a, PatternStr> {
    #[inline]
    fn from(p: &'a PatternStr) -> Cow<'a, PatternStr> {
        Cow::Borrowed(p)
    }
}

impl Display for PatternStr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct PatternString(pub(crate) String);

impl PatternString {
    #[inline]
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    #[inline]
    pub fn as_pattern_str(&self) -> &PatternStr {
        self.as_ref()
    }

    #[inline]
    pub fn from_components<'a, T>(iter: T) -> Result<Self, DuplicateGlob>
    where
        T: IntoIterator<Item = Component<'a>>,
    {
        iter.into_iter().collect()
    }

    #[inline]
    pub fn and<R>(mut self, other: R) -> Self
    where
        R: AsRef<RefStr>,
    {
        self._push(other.as_ref());
        self
    }

    #[inline]
    pub fn push<R>(&mut self, other: R)
    where
        R: AsRef<RefStr>,
    {
        self._push(other.as_ref())
    }

    fn _push(&mut self, other: &RefStr) {
        self.0.push('/');
        self.0.push_str(other.as_str());
    }

    #[inline]
    pub fn pop(&mut self) -> bool {
        match self.0.rfind('/') {
            None => false,
            Some(idx) => {
                self.0.truncate(idx);
                true
            }
        }
    }
}

impl Deref for PatternString {
    type Target = PatternStr;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.borrow()
    }
}

impl AsRef<PatternStr> for PatternString {
    #[inline]
    fn as_ref(&self) -> &PatternStr {
        self
    }
}

impl AsRef<str> for PatternString {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Borrow<PatternStr> for PatternString {
    #[inline]
    fn borrow(&self) -> &PatternStr {
        PatternStr::from_str(self.0.as_str())
    }
}

impl ToOwned for PatternStr {
    type Owned = PatternString;

    #[inline]
    fn to_owned(&self) -> Self::Owned {
        PatternString(self.0.to_owned())
    }
}

impl From<RefString> for PatternString {
    #[inline]
    fn from(rs: RefString) -> Self {
        Self(rs.into())
    }
}

impl<'a> From<&'a PatternString> for Cow<'a, PatternStr> {
    #[inline]
    fn from(p: &'a PatternString) -> Cow<'a, PatternStr> {
        Cow::Borrowed(p.as_ref())
    }
}

impl From<PatternString> for String {
    #[inline]
    fn from(p: PatternString) -> Self {
        p.0
    }
}

impl TryFrom<&str> for PatternString {
    type Error = check::Error;

    #[inline]
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        PatternStr::try_from_str(s).map(ToOwned::to_owned)
    }
}

impl TryFrom<String> for PatternString {
    type Error = check::Error;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        check::ref_format(CHECK_OPTS, s.as_str()).map(|()| PatternString(s))
    }
}

#[derive(Debug, Error)]
#[error("more than one '*' encountered")]
pub struct DuplicateGlob;

impl<'a> FromIterator<Component<'a>> for Result<PatternString, DuplicateGlob> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Component<'a>>,
    {
        use Component::*;

        let mut buf = String::new();
        let mut seen_glob = false;
        for c in iter {
            if let Glob(_) = c {
                if seen_glob {
                    return Err(DuplicateGlob);
                }

                seen_glob = true;
            }

            buf.push_str(c.as_str());
            buf.push('/');
        }
        buf.truncate(buf.len() - 1);

        Ok(PatternString(buf))
    }
}

impl Display for PatternString {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A fully-qualified refspec.
///
/// A refspec is qualified _iff_ it starts with "refs/" and has at least three
/// components. This implies that a [`QualifiedPattern`] ref has a category,
/// such as "refs/heads/*".
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct QualifiedPattern<'a>(pub(crate) Cow<'a, PatternStr>);

impl<'a> QualifiedPattern<'a> {
    pub fn from_patternstr(r: impl Into<Cow<'a, PatternStr>>) -> Option<Self> {
        Self::_from_patternstr(r.into())
    }

    fn _from_patternstr(r: Cow<'a, PatternStr>) -> Option<Self> {
        let mut iter = r.iter();
        match (iter.next()?, iter.next()?, iter.next()?) {
            ("refs", _, _) => Some(QualifiedPattern(r)),
            _ => None,
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    #[inline]
    pub fn join<'b, R>(&self, other: R) -> QualifiedPattern<'b>
    where
        R: AsRef<RefStr>,
    {
        QualifiedPattern(Cow::Owned(self.0.join(other)))
    }

    #[inline]
    pub fn to_namespaced(&'a self) -> Option<NamespacedPattern<'a>> {
        self.0.as_ref().into()
    }

    /// Add a namespace.
    ///
    /// Creates a new [`NamespacedPattern`] by prefxing `self` with
    /// `refs/namespaces/<ns>`.
    pub fn with_namespace<'b>(
        &self,
        ns: Component<'b>,
    ) -> Result<NamespacedPattern<'a>, DuplicateGlob> {
        PatternString::from_components(
            IntoIterator::into_iter([lit::Refs.into(), lit::Namespaces.into(), ns])
                .chain(self.components()),
        )
        .map(|pat| NamespacedPattern(Cow::Owned(pat)))
    }

    /// Like [`Self::non_empty_components`], but with string slices.
    pub fn non_empty_iter(&'a self) -> (&'a str, &'a str, &'a str, Iter<'a>) {
        let mut iter = self.iter();
        (
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter,
        )
    }

    /// Return the first three [`Component`]s, and a possibly empty iterator
    /// over the remaining ones.
    ///
    /// A qualified ref is guaranteed to have at least three components, which
    /// this method provides a witness of. This is useful eg. for pattern
    /// matching on the prefix.
    pub fn non_empty_components(
        &'a self,
    ) -> (Component<'a>, Component<'a>, Component<'a>, Components<'a>) {
        let mut cs = self.components();
        (
            cs.next().unwrap(),
            cs.next().unwrap(),
            cs.next().unwrap(),
            cs,
        )
    }

    #[inline]
    pub fn to_owned<'b>(&self) -> QualifiedPattern<'b> {
        QualifiedPattern(Cow::Owned(self.0.clone().into_owned()))
    }

    #[inline]
    pub fn into_owned<'b>(self) -> QualifiedPattern<'b> {
        QualifiedPattern(Cow::Owned(self.0.into_owned()))
    }

    #[inline]
    pub fn into_patternstring(self) -> PatternString {
        self.into()
    }
}

impl Deref for QualifiedPattern<'_> {
    type Target = PatternStr;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<PatternStr> for QualifiedPattern<'_> {
    #[inline]
    fn as_ref(&self) -> &PatternStr {
        self
    }
}

impl AsRef<str> for QualifiedPattern<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<Self> for QualifiedPattern<'_> {
    #[inline]
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Display for QualifiedPattern<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a> From<Qualified<'a>> for QualifiedPattern<'a> {
    #[inline]
    fn from(q: Qualified<'a>) -> Self {
        Self(Cow::Owned(q.into_refstring().into()))
    }
}

impl<'a> From<QualifiedPattern<'a>> for Cow<'a, PatternStr> {
    #[inline]
    fn from(q: QualifiedPattern<'a>) -> Self {
        q.0
    }
}

impl From<QualifiedPattern<'_>> for PatternString {
    #[inline]
    fn from(q: QualifiedPattern) -> Self {
        q.0.into_owned()
    }
}

/// A [`PatternString`] ref under a git namespace.
///
/// A ref is namespaced if it starts with "refs/namespaces/", another path
/// component, and "refs/". Eg.
///
///     refs/namespaces/xyz/refs/heads/main
///
/// Note that namespaces can be nested, so the result of
/// [`NamespacedPattern::strip_namespace`] may be convertible to a
/// [`NamespacedPattern`] again. For example:
///
/// ```no_run
/// let full = pattern!("refs/namespaces/a/refs/namespaces/b/refs/heads/*");
/// let namespaced = full.to_namespaced().unwrap();
/// let strip_first = namespaced.strip_namespace();
/// let nested = strip_first.namespaced().unwrap();
/// let strip_second = nested.strip_namespace();
///
/// assert_eq!("a", namespaced.namespace().as_str());
/// assert_eq!("b", nested.namespace().as_str());
/// assert_eq!("refs/namespaces/b/refs/heads/*", strip_first.as_str());
/// assert_eq!("refs/heads/*", strip_second.as_str());
/// ```
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct NamespacedPattern<'a>(Cow<'a, PatternStr>);

impl<'a> NamespacedPattern<'a> {
    pub fn namespace(&'a self) -> Component<'a> {
        self.components().nth(2).unwrap()
    }

    pub fn strip_namespace(&self) -> PatternString {
        PatternString::from_components(self.components().skip(3))
            .expect("BUG: NamespacedPattern was constructed with a duplicate glob")
    }

    pub fn strip_namespace_recursive(&self) -> PatternString {
        let mut strip = self.strip_namespace();
        while let Some(ns) = strip.to_namespaced() {
            strip = ns.strip_namespace();
        }
        strip
    }

    #[inline]
    pub fn to_owned<'b>(&self) -> NamespacedPattern<'b> {
        NamespacedPattern(Cow::Owned(self.0.clone().into_owned()))
    }

    #[inline]
    pub fn into_owned<'b>(self) -> NamespacedPattern<'b> {
        NamespacedPattern(Cow::Owned(self.0.into_owned()))
    }

    #[inline]
    pub fn into_qualified(self) -> QualifiedPattern<'a> {
        self.into()
    }
}

impl Deref for NamespacedPattern<'_> {
    type Target = PatternStr;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.borrow()
    }
}

impl AsRef<PatternStr> for NamespacedPattern<'_> {
    #[inline]
    fn as_ref(&self) -> &PatternStr {
        self.0.as_ref()
    }
}

impl AsRef<str> for NamespacedPattern<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Borrow<PatternStr> for NamespacedPattern<'_> {
    #[inline]
    fn borrow(&self) -> &PatternStr {
        PatternStr::from_str(self.0.as_str())
    }
}

impl<'a> From<Namespaced<'a>> for NamespacedPattern<'a> {
    #[inline]
    fn from(ns: Namespaced<'a>) -> Self {
        NamespacedPattern(Cow::Owned(ns.to_ref_string().into()))
    }
}

impl<'a> From<NamespacedPattern<'a>> for QualifiedPattern<'a> {
    #[inline]
    fn from(ns: NamespacedPattern<'a>) -> Self {
        Self(ns.0)
    }
}

impl<'a> From<&'a PatternStr> for Option<NamespacedPattern<'a>> {
    fn from(rs: &'a PatternStr) -> Self {
        let mut cs = rs.iter();
        match (cs.next()?, cs.next()?, cs.next()?, cs.next()?) {
            ("refs", "namespaces", _, "refs") => Some(NamespacedPattern(Cow::from(rs))),

            _ => None,
        }
    }
}

impl Display for NamespacedPattern<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A Git [refspec].
///
/// **Note** that this is a simplified version of a [refspec] where
/// the `src` and `dst` are required and there is no way to construct
/// a negative refspec, e.g. `^refs/heads/no-thanks`.
///
/// [refspec]: https://git-scm.com/book/en/v2/Git-Internals-The-Refspec
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refspec<T, U> {
    pub src: T,
    pub dst: U,
    pub force: bool,
}

impl<T, U> fmt::Display for Refspec<T, U>
where
    T: fmt::Display,
    U: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.force {
            f.write_char('+')?;
        }
        write!(f, "{}:{}", self.src, self.dst)
    }
}

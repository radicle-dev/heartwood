use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::ops::BitOr;

use super::{Color, Paint};

#[derive(Default, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Clone)]
pub struct Property(u8);

impl Property {
    pub const BOLD: Self = Property(1 << 0);
    pub const DIM: Self = Property(1 << 1);
    pub const ITALIC: Self = Property(1 << 2);
    pub const UNDERLINE: Self = Property(1 << 3);
    pub const BLINK: Self = Property(1 << 4);
    pub const INVERT: Self = Property(1 << 5);
    pub const HIDDEN: Self = Property(1 << 6);
    pub const STRIKETHROUGH: Self = Property(1 << 7);

    pub const fn new() -> Self {
        Property(0)
    }

    #[inline(always)]
    pub const fn contains(self, other: Property) -> bool {
        (other.0 & self.0) == other.0
    }

    #[inline(always)]
    pub fn set(&mut self, other: Property) {
        self.0 |= other.0;
    }

    #[inline(always)]
    pub fn iter(self) -> Iter {
        Iter {
            index: 0,
            properties: self,
        }
    }
}

impl BitOr for Property {
    type Output = Self;

    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self {
        Property(self.0 | rhs.0)
    }
}

pub struct Iter {
    index: u8,
    properties: Property,
}

impl Iterator for Iter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < 8 {
            let index = self.index;
            self.index += 1;

            if self.properties.contains(Property(1 << index)) {
                return Some(index as usize);
            }
        }

        None
    }
}

/// Represents a set of styling options.
#[repr(packed)]
#[derive(Default, Debug, Eq, Ord, PartialOrd, Copy, Clone)]
pub struct Style {
    pub(crate) foreground: Color,
    pub(crate) background: Color,
    pub(crate) properties: Property,
    pub(crate) wrap: bool,
}

impl PartialEq for Style {
    fn eq(&self, other: &Style) -> bool {
        self.foreground == other.foreground
            && self.background == other.background
            && self.properties == other.properties
    }
}

impl Hash for Style {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.foreground.hash(state);
        self.background.hash(state);
        self.properties.hash(state);
    }
}

#[inline]
fn write_spliced<T: Display>(c: &mut bool, f: &mut dyn fmt::Write, t: T) -> fmt::Result {
    if *c {
        write!(f, ";{t}")
    } else {
        *c = true;
        write!(f, "{t}")
    }
}

impl Style {
    /// Default style with the foreground set to `color` and no other set
    /// properties.
    #[inline]
    pub const fn new(color: Color) -> Style {
        // Avoiding `Default::default` since unavailable as `const`
        Self {
            foreground: color,
            background: Color::Unset,
            properties: Property::new(),
            wrap: false,
        }
    }

    /// Sets the foreground to `color`.
    #[inline]
    pub const fn fg(mut self, color: Color) -> Style {
        self.foreground = color;
        self
    }

    /// Sets the background to `color`.
    #[inline]
    pub const fn bg(mut self, color: Color) -> Style {
        self.background = color;
        self
    }

    /// Sets `self` to be wrapping.
    ///
    /// A wrapping `Style` converts all color resets written out by the internal
    /// value to the styling of itself. This allows for seamless color wrapping
    /// of other colored text.
    ///
    /// # Performance
    ///
    /// In order to wrap an internal value, the internal value must first be
    /// written out to a local buffer and examined. As a result, displaying a
    /// wrapped value is likely to result in a heap allocation and copy.
    #[inline]
    pub const fn wrap(mut self) -> Style {
        self.wrap = true;
        self
    }

    pub fn bold(mut self) -> Self {
        self.properties.set(Property::BOLD);
        self
    }

    pub fn dim(mut self) -> Self {
        self.properties.set(Property::DIM);
        self
    }

    pub fn italic(mut self) -> Self {
        self.properties.set(Property::ITALIC);
        self
    }

    pub fn underline(mut self) -> Self {
        self.properties.set(Property::UNDERLINE);
        self
    }

    pub fn invert(mut self) -> Self {
        self.properties.set(Property::INVERT);
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.properties.set(Property::STRIKETHROUGH);
        self
    }

    /// Constructs a new `Paint` structure that encapsulates `item` with the
    /// style set to `self`.
    #[inline]
    pub fn paint<T>(self, item: T) -> Paint<T> {
        Paint::new(item).with_style(self)
    }

    /// Returns the foreground color of `self`.
    #[inline]
    pub const fn fg_color(&self) -> Color {
        self.foreground
    }

    /// Returns the foreground color of `self`.
    #[inline]
    pub const fn bg_color(&self) -> Color {
        self.background
    }

    /// Returns `true` if `self` is wrapping.
    #[inline]
    pub const fn is_wrapping(&self) -> bool {
        self.wrap
    }

    #[inline(always)]
    fn is_plain(&self) -> bool {
        self == &Style::default()
    }

    /// Writes the ANSI code prefix for the currently set styles.
    ///
    /// This method is intended to be used inside of [`fmt::Display`] and
    /// [`fmt::Debug`] implementations for custom or specialized use-cases. Most
    /// users should use [`Paint`] for all painting needs.
    ///
    /// This method writes the ANSI code prefix irrespective of whether painting
    /// is currently enabled or disabled. To write the prefix only if painting
    /// is enabled, condition a call to this method on [`Paint::is_enabled()`].
    pub fn fmt_prefix(&self, f: &mut dyn fmt::Write) -> fmt::Result {
        // A user may just want a code-free string when no styles are applied.
        if self.is_plain() {
            return Ok(());
        }

        let mut splice = false;
        write!(f, "\x1B[")?;

        for i in self.properties.iter() {
            let k = if i >= 5 { i + 2 } else { i + 1 };
            write_spliced(&mut splice, f, k)?;
        }

        if self.background != Color::Unset {
            write_spliced(&mut splice, f, "4")?;
            self.background.ansi_fmt(f)?;
        }

        if self.foreground != Color::Unset {
            write_spliced(&mut splice, f, "3")?;
            self.foreground.ansi_fmt(f)?;
        }

        // All the codes end with an `m`.
        write!(f, "m")
    }

    /// Writes the ANSI code suffix for the currently set styles.
    ///
    /// This method is intended to be used inside of [`fmt::Display`] and
    /// [`fmt::Debug`] implementations for custom or specialized use-cases. Most
    /// users should use [`Paint`] for all painting needs.
    ///
    /// This method writes the ANSI code suffix irrespective of whether painting
    /// is currently enabled or disabled. To write the suffix only if painting
    /// is enabled, condition a call to this method on [`Paint::is_enabled()`].
    pub fn fmt_suffix(&self, f: &mut dyn fmt::Write) -> fmt::Result {
        if self.is_plain() {
            return Ok(());
        }
        write!(f, "\x1B[0m")
    }
}

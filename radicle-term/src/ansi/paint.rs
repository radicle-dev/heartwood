use std::io::IsTerminal as _;
use std::sync;
use std::sync::atomic::AtomicBool;
use std::{fmt, io};

use unicode_width::UnicodeWidthStr;

use super::color::Color;
use super::style::{Property, Style};

/// Whether paint styling is enabled or not.
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Whether paint styling should be forced.
static FORCED: AtomicBool = AtomicBool::new(false);

/// A structure encapsulating an item and styling.
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Clone)]
pub struct Paint<T> {
    pub item: T,
    pub style: Style,
}

impl Paint<&str> {
    /// Return plain content.
    pub fn content(&self) -> &str {
        self.item
    }
}

impl Paint<String> {
    /// Return plain content.
    pub fn content(&self) -> &str {
        self.item.as_str()
    }
}

impl<T> From<T> for Paint<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Paint<String> {
    fn from(item: &str) -> Self {
        Self::new(item.to_string())
    }
}

impl From<Paint<&str>> for Paint<String> {
    fn from(paint: Paint<&str>) -> Self {
        Self {
            item: paint.item.to_owned(),
            style: paint.style,
        }
    }
}

impl<T> Paint<T> {
    /// Constructs a new `Paint` structure encapsulating `item` with no set
    /// styling.
    #[inline]
    pub const fn new(item: T) -> Paint<T> {
        Paint {
            item,
            style: Style {
                foreground: Color::Unset,
                background: Color::Unset,
                properties: Property::new(),
                wrap: false,
            },
        }
    }

    /// Constructs a new _wrapping_ `Paint` structure encapsulating `item` with
    /// default styling.
    ///
    /// A wrapping `Paint` converts all color resets written out by the internal
    /// value to the styling of itself. This allows for seamless color wrapping
    /// of other colored text.
    ///
    /// # Performance
    ///
    /// In order to wrap an internal value, the internal value must first be
    /// written out to a local buffer and examined. As a result, displaying a
    /// wrapped value is likely to result in a heap allocation and copy.
    #[inline]
    pub const fn wrapping(item: T) -> Paint<T> {
        Paint::new(item).wrap()
    }

    /// Constructs a new `Paint` structure encapsulating `item` with the
    /// foreground color set to the RGB color `r`, `g`, `b`.
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8, item: T) -> Paint<T> {
        Paint::new(item).fg(Color::RGB(r, g, b))
    }

    /// Constructs a new `Paint` structure encapsulating `item` with the
    /// foreground color set to the fixed 8-bit color `color`.
    #[inline]
    pub const fn fixed(color: u8, item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Fixed(color))
    }

    pub const fn red(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Red)
    }

    pub const fn black(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Black)
    }

    pub const fn yellow(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Yellow)
    }

    pub const fn green(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Green)
    }

    pub const fn cyan(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Cyan)
    }

    pub const fn blue(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Blue)
    }

    pub const fn magenta(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::Magenta)
    }

    pub const fn white(item: T) -> Paint<T> {
        Paint::new(item).fg(Color::White)
    }

    /// Retrieves the style currently set on `self`.
    #[inline]
    pub const fn style(&self) -> Style {
        self.style
    }

    /// Retrieves a borrow to the inner item.
    #[inline]
    pub const fn inner(&self) -> &T {
        &self.item
    }

    /// Sets the style of `self` to `style`.
    #[inline]
    pub fn with_style(mut self, style: Style) -> Paint<T> {
        self.style = style;
        self
    }

    /// Makes `self` a _wrapping_ `Paint`.
    ///
    /// A wrapping `Paint` converts all color resets written out by the internal
    /// value to the styling of itself. This allows for seamless color wrapping
    /// of other colored text.
    ///
    /// # Performance
    ///
    /// In order to wrap an internal value, the internal value must first be
    /// written out to a local buffer and examined. As a result, displaying a
    /// wrapped value is likely to result in a heap allocation and copy.
    #[inline]
    pub const fn wrap(mut self) -> Paint<T> {
        self.style.wrap = true;
        self
    }

    /// Sets the foreground to `color`.
    #[inline]
    pub const fn fg(mut self, color: Color) -> Paint<T> {
        self.style.foreground = color;
        self
    }

    /// Sets the background to `color`.
    #[inline]
    pub const fn bg(mut self, color: Color) -> Paint<T> {
        self.style.background = color;
        self
    }

    pub fn bold(mut self) -> Self {
        self.style.properties.set(Property::BOLD);
        self
    }

    pub fn dim(mut self) -> Self {
        self.style.properties.set(Property::DIM);
        self
    }

    pub fn italic(mut self) -> Self {
        self.style.properties.set(Property::ITALIC);
        self
    }

    pub fn underline(mut self) -> Self {
        self.style.properties.set(Property::UNDERLINE);
        self
    }

    pub fn invert(mut self) -> Self {
        self.style.properties.set(Property::INVERT);
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.style.properties.set(Property::STRIKETHROUGH);
        self
    }

    pub fn blink(mut self) -> Self {
        self.style.properties.set(Property::BLINK);
        self
    }

    pub fn hidden(mut self) -> Self {
        self.style.properties.set(Property::HIDDEN);
        self
    }
}

impl<T: UnicodeWidthStr> UnicodeWidthStr for Paint<T> {
    fn width(&self) -> usize {
        self.item.width()
    }

    fn width_cjk(&self) -> usize {
        self.item.width_cjk()
    }
}

impl<T: fmt::Display> fmt::Display for Paint<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if Paint::is_enabled() && self.style.wrap {
            let mut prefix = String::new();
            prefix.push_str("\x1B[0m");
            self.style.fmt_prefix(&mut prefix)?;
            self.style.fmt_prefix(f)?;

            let item = format!("{}", self.item).replace("\x1B[0m", &prefix);
            fmt::Display::fmt(&item, f)?;
            self.style.fmt_suffix(f)
        } else if Paint::is_enabled() {
            self.style.fmt_prefix(f)?;
            fmt::Display::fmt(&self.item, f)?;
            self.style.fmt_suffix(f)
        } else {
            fmt::Display::fmt(&self.item, f)
        }
    }
}

impl Paint<()> {
    /// Returns `true` if coloring is enabled and `false` otherwise.
    pub fn is_enabled() -> bool {
        if FORCED.load(sync::atomic::Ordering::SeqCst) {
            return true;
        }
        let clicolor = anstyle_query::clicolor();
        let clicolor_enabled = clicolor.unwrap_or(false);
        let clicolor_disabled = !clicolor.unwrap_or(true);
        let is_terminal = io::stdout().is_terminal();
        let is_enabled = ENABLED.load(sync::atomic::Ordering::SeqCst);

        is_terminal
            && is_enabled
            && !anstyle_query::no_color()
            && !clicolor_disabled
            && (anstyle_query::term_supports_color() || clicolor_enabled || anstyle_query::is_ci())
            || anstyle_query::clicolor_force()
    }

    /// Enable paint styling.
    pub fn enable() {
        ENABLED.store(true, sync::atomic::Ordering::SeqCst);
    }

    /// Force paint styling.
    /// Useful when you want to output colors to a non-TTY.
    pub fn force(force: bool) {
        FORCED.store(force, sync::atomic::Ordering::SeqCst);
    }

    /// Disable paint styling.
    pub fn disable() {
        ENABLED.store(false, sync::atomic::Ordering::SeqCst);
    }
}

/// Shorthand for [`Paint::new`].
pub fn paint<T>(item: T) -> Paint<T> {
    Paint::new(item)
}

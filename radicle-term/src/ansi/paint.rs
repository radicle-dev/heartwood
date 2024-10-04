use std::io::IsTerminal as _;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::sync::atomic::{AtomicBool, AtomicI32};
use std::{fmt, sync};

use once_cell::sync::Lazy;

use super::color::Color;
use super::display::{Context, Display};
use super::display_with;
use super::style::{Property, Style};

/// What file is used for text output.
static TERMINAL: AtomicI32 = AtomicI32::new(libc::STDOUT_FILENO);
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

impl From<Paint<String>> for String {
    fn from(value: Paint<String>) -> Self {
        value.item
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

/*
impl<T: fmt::Display> Display for Paint<T> {
    fn fmt_with(&self, f: &mut fmt::Formatter<'_>, ctx: &Context) -> fmt::Result {
        if ctx.ansi && self.style.wrap {
            let mut prefix = String::new();
            prefix.push_str("\x1B[0m");
            self.style.fmt_prefix(&mut prefix)?;
            self.style.fmt_prefix(f)?;

            let item = format!("{}", self.item).replace("\x1B[0m", &prefix);
            fmt::Display::fmt(&item, f)?;
            self.style.fmt_suffix(f)
        } else if ctx.ansi {
            self.style.fmt_prefix(f)?;
            fmt::Display::fmt(&self.item, f)?;
            self.style.fmt_suffix(f)
        } else {
            fmt::Display::fmt(&self.item, f)
        }
    }
}
*/

impl<T: Display> Display for Paint<T> {
    fn fmt_with(&self, f: &mut fmt::Formatter<'_>, ctx: &Context) -> fmt::Result {
        if ctx.ansi && self.style.wrap {
            let mut prefix = String::new();
            prefix.push_str("\x1B[0m");
            self.style.fmt_prefix(&mut prefix)?;
            self.style.fmt_prefix(f)?;

            let item = display_with(&self
                .item, ctx)
                .to_string()
                .replace("\x1B[0m", &prefix);
            fmt::Display::fmt(&item, f)?;
            self.style.fmt_suffix(f)
        } else if ctx.ansi {
            self.style.fmt_prefix(f)?;
            self.item.fmt_with(f, ctx)?;
            self.style.fmt_suffix(f)
        } else {
            self.item.fmt_with(f, ctx)
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
        let terminal = TERMINAL.load(sync::atomic::Ordering::SeqCst);
        let is_terminal = unsafe { BorrowedFd::borrow_raw(terminal).is_terminal() };
        let is_enabled = ENABLED.load(sync::atomic::Ordering::SeqCst);

        is_terminal
            && is_enabled
            && !anstyle_query::no_color()
            && !clicolor_disabled
            && (anstyle_query::term_supports_color() || clicolor_enabled || anstyle_query::is_ci())
            || anstyle_query::clicolor_force()
    }

    /// Check 24-bit RGB color support.
    pub fn truecolor() -> bool {
        static TRUECOLOR: Lazy<bool> = Lazy::new(anstyle_query::term_supports_color);
        *TRUECOLOR
    }

    /// Enable paint styling.
    pub fn enable() {
        ENABLED.store(true, sync::atomic::Ordering::SeqCst);
    }

    /// Set the terminal we are writing to. This influences the logic that checks whether or not to
    /// include colors.
    pub fn set_terminal(fd: impl AsRawFd) {
        TERMINAL.store(fd.as_raw_fd(), sync::atomic::Ordering::SeqCst);
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

/// An object filled with a background color.
#[derive(Debug, Clone)]
pub struct Filled<T> {
    pub item: T,
    pub color: Color,
}

impl<T: fmt::Display> Display for Filled<T> {
    fn fmt_with(&self, f: &mut fmt::Formatter<'_>, ctx: &Context) -> fmt::Result {
        Paint::wrapping(&self.item).bg(self.color).fmt_with(f, ctx)
    }
}

/// Shorthand for [`Paint::new`].
pub fn paint<T>(item: T) -> Paint<T> {
    Paint::new(item)
}

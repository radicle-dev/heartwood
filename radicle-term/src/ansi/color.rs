use std::fmt;

use super::{Paint, Style};

/// An enum representing an ANSI color code.
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Clone)]
pub enum Color {
    /// No color has been set. Nothing is changed when applied.
    #[default]
    Unset,
    /// Black #0 (foreground code `30`, background code `40`).
    Black,
    /// Red: #1 (foreground code `31`, background code `41`).
    Red,
    /// Green: #2 (foreground code `32`, background code `42`).
    Green,
    /// Yellow: #3 (foreground code `33`, background code `43`).
    Yellow,
    /// Blue: #4 (foreground code `34`, background code `44`).
    Blue,
    /// Magenta: #5 (foreground code `35`, background code `45`).
    Magenta,
    /// Cyan: #6 (foreground code `36`, background code `46`).
    Cyan,
    /// White: #7 (foreground code `37`, background code `47`).
    White,
    /// A color number from 0 to 255, for use in 256-color terminals.
    Fixed(u8),
    /// A 24-bit RGB color, as specified by ISO-8613-3.
    RGB(u8, u8, u8),
}

impl Color {
    /// Constructs a new `Paint` structure that encapsulates `item` with the
    /// foreground color set to the color `self`.
    #[inline]
    pub fn paint<T>(self, item: T) -> Paint<T> {
        Paint::new(item).fg(self)
    }

    /// Constructs a new `Style` structure with the foreground color set to the
    /// color `self`.
    #[inline]
    pub const fn style(self) -> Style {
        Style::new(self)
    }

    pub fn complimentary(&self) -> Option<Color> {
        match *self {
            Color::Unset => Some(Color::Unset),
            Color::White => Some(Color::Black),
            Color::RGB(r, g, b) => Some(Color::RGB(u8::MAX - r, u8::MAX - g, u8::MAX - b)),
            _ => None,
        }
    }

    pub(crate) fn ansi_fmt(&self, f: &mut dyn fmt::Write) -> fmt::Result {
        match *self {
            Color::Unset => Ok(()),
            Color::Black => write!(f, "0"),
            Color::Red => write!(f, "1"),
            Color::Green => write!(f, "2"),
            Color::Yellow => write!(f, "3"),
            Color::Blue => write!(f, "4"),
            Color::Magenta => write!(f, "5"),
            Color::Cyan => write!(f, "6"),
            Color::White => write!(f, "7"),
            Color::Fixed(num) => write!(f, "8;5;{num}"),
            Color::RGB(r, g, b) => write!(f, "8;2;{r};{g};{b}"),
        }
    }
}

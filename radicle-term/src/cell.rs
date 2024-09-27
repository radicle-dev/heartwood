use std::fmt;

use super::{Color, Display, Filled, Line, Paint};

use unicode_display_width as unicode;
use unicode_segmentation::UnicodeSegmentation as _;

/// Text that can be displayed on the terminal, measured, truncated and padded.
pub trait Cell<'a>: Display<'a> {
    /// Type after truncation.
    type Truncated: Cell<'a>;

    /// Type after padding.
    type Padded: Cell<'a>;

    /// Cell display width in number of terminal columns.
    fn width(&self) -> usize;

    /// Background color of cell.
    fn background(&self) -> Color {
        Color::Unset
    }

    /// Truncate cell if longer than given width. Shows the delimiter if truncated.
    #[must_use]
    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated;

    /// Pad the cell so that it is the given width, while keeping the content left-aligned.
    #[must_use]
    fn pad(&self, width: usize) -> Self::Padded;
}

impl<'a, T: Cell<'a>> Cell<'a> for Paint<T> {
    type Truncated = Paint<T::Truncated>;
    type Padded = Paint<T::Padded>;

    fn width(&self) -> usize {
        self.item.width()
    }

    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated {
        Paint {
            item: self.item.truncate(width, delim),
            style: self.style,
        }
    }

    fn pad(&self, width: usize) -> Self::Padded {
        Paint {
            item: self.item.pad(width),
            style: self.style,
        }
    }
}

/*
impl Cell<'_> for Paint<String> {
    type Truncated = Self;
    type Padded = Self;

    fn width(&self) -> usize {
        self.content().graphemes(true)
            .map(|g| unicode::width(g) as usize)
            .sum()
    }

    fn background(&self) -> Color {
        self.style.background
    }

    fn truncate(&self, width: usize, delim: &str) -> Self {
        Self {
            item: self.item.truncate(width, delim),
            style: self.style,
        }
    }

    fn pad(&self, width: usize) -> Self {
        Self {
            item: self.item.pad(width),
            style: self.style,
        }
    }
}
*/

impl Cell<'_> for Line {
    type Truncated = Line;
    type Padded = Line;

    fn width(&self) -> usize {
        Line::width(self)
    }

    fn pad(&self, width: usize) -> Self::Padded {
        let mut line = self.clone();
        Line::pad(&mut line, width);
        line
    }

    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated {
        let mut line = self.clone();
        Line::truncate(&mut line, width, delim);
        line
    }
}

/*
impl Cell<'_> for Paint<&str> {
    type Truncated = Paint<String>;
    type Padded = Paint<String>;

    fn width(&self) -> usize {
        Cell::width(&self.item)
    }

    fn background(&self) -> Color {
        self.style.background
    }

    fn truncate(&self, width: usize, delim: &str) -> Paint<String> {
        let truncated = {
        if width < Cell::width(self) {
            let d = Cell::width(&delim);
            if width < d {
                // If we can't even fit the delimiter, just return an empty string.
                String::new()
            } else {
            // Find the unicode byte boundary where the display width is the largest,
            // while being smaller than the given max width.
            let mut cols = 0; // Number of visual columns we need.
            let mut boundary = 0; // Boundary in bytes.
            for g in self.item.graphemes(true) {
                let c = Cell::width(&g);
                if cols + c + d > width {
                    break;
                }
                boundary += g.len();
                cols += c;
            }
            // Don't add the delimiter if we just trimmed whitespace.
            if self.item[boundary..].trim().is_empty() {
                self.item[..boundary + 1].to_owned()
            } else {
                format!("{}{delim}", &self.item[..boundary])
            }
        }
        } else {
            self.item.to_owned()
        }
    };
        Paint {
            item: truncated,
            style: self.style,
        }
    }

    fn pad(&self, width: usize) -> Paint<String> {
        Paint {
            item: self.item.pad(width),
            style: self.style,
        }
    }
}
*/

impl Cell<'_> for String {
    type Truncated = Self;
    type Padded = Self;

    fn width(&self) -> usize {
        Cell::width(&self.as_str())
    }

    fn truncate(&self, width: usize, delim: &str) -> Self {
        self.as_str().truncate(width, delim)
    }

    fn pad(&self, width: usize) -> Self {
        self.as_str().pad(width)
    }
}

impl Cell<'_> for &str {
    type Truncated = String;
    type Padded = String;

    fn width(&self) -> usize {
        self.graphemes(true)
            .map(|g| unicode::width(g) as usize)
            .sum()
    }

    fn truncate(&self, width: usize, delim: &str) -> String {
        if width < Cell::width(self) {
            let d = Cell::width(&delim);
            if width < d {
                // If we can't even fit the delimiter, just return an empty string.
                return String::new();
            }
            // Find the unicode byte boundary where the display width is the largest,
            // while being smaller than the given max width.
            let mut cols = 0; // Number of visual columns we need.
            let mut boundary = 0; // Boundary in bytes.
            for g in self.graphemes(true) {
                let c = Cell::width(&g);
                if cols + c + d > width {
                    break;
                }
                boundary += g.len();
                cols += c;
            }
            // Don't add the delimiter if we just trimmed whitespace.
            if self[boundary..].trim().is_empty() {
                self[..boundary + 1].to_owned()
            } else {
                format!("{}{delim}", &self[..boundary])
            }
        } else {
            String::from(*self)
        }
    }

    fn pad(&self, max: usize) -> String {
        let width = Cell::width(self);

        if width < max {
            format!("{self}{}", " ".repeat(max - width))
        } else {
            String::from(*self)
        }
    }
}

/*
impl Cell<'_> for str {
    type Truncated = String;
    type Padded = String;

    fn width(&self) -> usize {
        self.graphemes(true)
            .map(|g| unicode::width(g) as usize)
            .sum()
    }

    fn truncate(&self, width: usize, delim: &str) -> String {
        if width < Cell::width(&self) {
            let d = Cell::width(&delim);
            if width < d {
                // If we can't even fit the delimiter, just return an empty string.
                return String::new();
            }
            // Find the unicode byte boundary where the display width is the largest,
            // while being smaller than the given max width.
            let mut cols = 0; // Number of visual columns we need.
            let mut boundary = 0; // Boundary in bytes.
            for g in self.graphemes(true) {
                let c = Cell::width(g);
                if cols + c + d > width {
                    break;
                }
                boundary += g.len();
                cols += c;
            }
            // Don't add the delimiter if we just trimmed whitespace.
            if self[boundary..].trim().is_empty() {
                self[..boundary + 1].to_owned()
            } else {
                format!("{}{delim}", &self[..boundary])
            }
        } else {
            self.to_owned()
        }
    }

    fn pad(&self, max: usize) -> String {
        let width = Cell::width(&self);

        if width < max {
            format!("{self}{}", " ".repeat(max - width))
        } else {
            self.to_owned()
        }
    }
}

impl<'a, T: Cell<'a> + ?Sized + fmt::Display> Cell<'a> for &T {
    type Truncated = T::Truncated;
    type Padded = T::Padded;

    fn width(&self) -> usize {
        T::width(self)
    }

    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated {
        T::truncate(self, width, delim)
    }

    fn pad(&self, width: usize) -> Self::Padded {
        T::pad(self, width)
    }
}
*/

impl<'a, T: Cell<'a> + fmt::Display> Cell<'a> for Filled<T> {
    type Truncated = T::Truncated;
    type Padded = T::Padded;

    fn width(&self) -> usize {
        T::width(&self.item)
    }

    fn background(&self) -> Color {
        self.color
    }

    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated {
        T::truncate(&self.item, width, delim)
    }

    fn pad(&self, width: usize) -> Self::Padded {
        T::pad(&self.item, width)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_width() {
        assert_eq!(unicode_display_width::width("❤️"), 2);
        assert_eq!(unicode_display_width::width("🪵"), 2);
    }
}
